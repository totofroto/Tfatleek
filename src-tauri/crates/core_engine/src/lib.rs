pub mod extractor;
pub mod transactions;
pub mod paperless_bridge;
pub mod settings;
pub use crypto_dedup::{run_dedup_scan, ScanResult};
use database::{DbManager, FileRecord};
use local_ai::{request_file_classification, FileMetadataPayload, AiClassificationResult};
use std::path::Path;
use tauri::{Emitter, Manager};
use serde::{Serialize, Deserialize};
use crate::transactions::SafeFileSystemEngine;
use crate::settings::SettingsManager;
use futures::stream::{self, StreamExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FamilyMember {
    pub key: String,
    pub full_name: String,
    pub birth_date: String,
    pub role: String,
}

// Provide the baseline default presets specified by the system owner
pub fn get_default_family_presets() -> Vec<FamilyMember> {
    vec![
        FamilyMember { key: "tareg".to_string(), full_name: "Tareg Mohamed Ahmed Shek".to_string(), birth_date: "05.10.1978".to_string(), role: "Father/Owner".to_string() },
        FamilyMember { key: "miluda".to_string(), full_name: "Miluda Bashir Shek".to_string(), birth_date: "24.08.1990".to_string(), role: "Mother".to_string() },
        FamilyMember { key: "fatima".to_string(), full_name: "Fatima Shek".to_string(), birth_date: "12.05.2015".to_string(), role: "Daughter".to_string() },
        FamilyMember { key: "sama".to_string(), full_name: "Sama Shek".to_string(), birth_date: "".to_string(), role: "Daughter".to_string() },
    ]
}

pub enum CoreCategory {
    Steuererklaerung,
    MedizinischePraxis,
    FinanzenUndBanken,
    PersonalCore,
}

impl CoreCategory {
    pub fn as_german_str(&self) -> &'static str {
        match self {
            CoreCategory::Steuererklaerung => "Steuererklaerung",
            CoreCategory::MedizinischePraxis => "Medizinische_Praxis",
            CoreCategory::FinanzenUndBanken => "Finanzen_und_Banken",
            CoreCategory::PersonalCore => "Persoenlicher_Kern",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    pub current: usize,
    pub total: usize,
    pub percentage: f32,
    pub current_file: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum IngestionContext {
    #[serde(rename = "PRIVATE")]
    Private,
    #[serde(rename = "OTHERS")]
    Others,
}

#[derive(Serialize, Deserialize)]
pub struct IngestionManifest {
    #[serde(rename = "originalPath")]
    pub original_path: String,
    #[serde(rename = "suggestedName")]
    pub suggested_name: String,
    #[serde(rename = "identifiedCategory")]
    pub identified_category: String,
    #[serde(rename = "detectedDate")]
    pub detected_date: String,
    #[serde(rename = "suggestedTargetTree")]
    pub suggested_target_tree: String,
    #[serde(rename = "isTaxRelevant")]
    pub is_tax_relevant: bool,
    #[serde(rename = "identifiedMember")]
    pub identified_member: Option<String>,
    #[serde(rename = "isUnsorted")]
    pub is_unsorted: bool,
}

fn current_year_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}", 1970u64 + secs / 31_557_600u64)
}

pub fn build_rational_path(base_root: &std::path::Path, category: &str, file_date: &str) -> std::path::PathBuf {
    let year = if file_date.len() >= 4 {
        file_date[0..4].to_string()
    } else {
        current_year_string()
    };
    base_root.join(category).join(year)
}

pub fn unsorted_path(base_root: &std::path::Path) -> std::path::PathBuf {
    base_root.join("_Unsorted")
}

pub async fn execute_batch_organization<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    root_scan_path: &str,
    db_path: &str,
) -> Result<String, String> {
    let target_lower = root_scan_path.to_lowercase();
    if target_lower == "/users/taregahmed/desktop" || target_lower == "/users/taregahmed/documents" || target_lower == "/" {
        return Err("Error: Direct root directory targeting is restricted for system safety. Please target a specific subfolder.".to_string());
    }

    let config_dir = app.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let settings_mgr = SettingsManager::new(&config_dir);
    let protected_paths = {
        let settings = settings_mgr.current.read().unwrap();
        settings.preset_paths.values().cloned().collect::<Vec<String>>()
    };
    let excluded_folders = {
        let settings = settings_mgr.current.read().unwrap();
        settings.excluded_folders.clone()
    };
    let papers_output_base = settings_mgr.papers_output_base();
    let (ollama_url, ollama_model, gemini_key) = {
        let settings = settings_mgr.current.read().unwrap();
        (
            settings.ollama_base_url.clone(),
            settings.ollama_model.clone(),
            settings.gemini_api_key.clone(),
        )
    };

    let db = DbManager::init(db_path).map_err(|e| e.to_string())?;
    
    // 1. Gather all file rows currently indexed in our SQLite database
    let mut files_to_process = Vec::new();
    if let Ok(conn_lock) = db.conn.lock() {
        let mut stmt = conn_lock.prepare("SELECT file_path FROM file_index").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
        for row in rows {
            if let Ok(file_path) = row {
                // Filter out excluded folders
                let is_excluded = excluded_folders.iter().any(|ex| file_path.contains(ex));
                if !is_excluded {
                    files_to_process.push(file_path);
                }
            }
        }
    }

    let total_files = files_to_process.len();
    if total_files == 0 {
        return Ok("No index files found in cache database state.".to_string());
    }

    let batch_id = format!("batch_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
    let completed_count = Arc::new(AtomicUsize::new(0));

    // 2. Process records with controlled concurrency to maximize GPU utilization
    let stream = stream::iter(files_to_process.into_iter())
        .map(|file_path| {
            let app = app.clone();
            let db_path = db_path.to_string();
            let ollama_url = ollama_url.clone();
            let ollama_model = ollama_model.clone();
            let gemini_key = gemini_key.clone();
            let completed_count = completed_count.clone();
            let protected_paths = protected_paths.clone();
            let fs_engine = SafeFileSystemEngine::new(&db_path, protected_paths);
            let papers_output_base = papers_output_base.clone();
            let batch_id = batch_id.clone();

            async move {
                let path = std::path::Path::new(&file_path);
                let ext = path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                // Non-negotiable high-value document targets
                let approved_white_list = ["pdf", "docx", "doc", "txt", "log", "dcm", "dicom"];

                let result = if !approved_white_list.contains(&ext.as_str()) {
                    Err("SKIPPED_BY_WHITELIST".to_string())
                } else {
                    run_ai_classification(&file_path, &db_path, &ollama_url, &ollama_model, &gemini_key, IngestionContext::Private).await
                };
                
                match result {
                    Ok(ai_decision) => {
                        let output_base = std::path::Path::new(&papers_output_base);
                        if ai_decision.confidence_score >= 0.5 {
                            let date_str = format!("{}-01-01", current_year_string());
                            let combined_category = if ai_decision.category.is_empty() {
                                ai_decision.suggested_subfolder.clone()
                            } else {
                                format!("{}/{}", ai_decision.category, ai_decision.suggested_subfolder)
                            };
                            let target_dir = build_rational_path(output_base, &combined_category, &date_str);
                            let _ = std::fs::create_dir_all(&target_dir);
                            let final_destination_path = target_dir.join(&ai_decision.new_clean_name);
                            let _ = fs_engine.execute_safe_move(&batch_id, &file_path, &final_destination_path.to_string_lossy());
                        } else {
                            let unsorted_dir = unsorted_path(output_base);
                            let _ = std::fs::create_dir_all(&unsorted_dir);
                            let original_name = std::path::Path::new(&file_path)
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "unknown".to_string());
                            let dest = unsorted_dir.join(&original_name);
                            let _ = fs_engine.execute_safe_move(&batch_id, &file_path, &dest.to_string_lossy());
                        }
                    }
                    Err(_) => {} // Keep processing remaining files if one fails
                }

                let current = completed_count.fetch_add(1, Ordering::SeqCst) + 1;
                
                // Broadcast active real-time metrics back to React via Tauri v2 Emitter traits
                // Moved to AFTER relocation commit to ensure UI state syncs with disk/DB state
                let progress = ProgressPayload {
                    current,
                    total: total_files,
                    percentage: (current as f32 / total_files as f32) * 100.0,
                    current_file: path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                };
                let _ = app.emit("scan-progress", &progress);
            }
        })
        .buffer_unordered(4);

    stream.collect::<Vec<()>>().await;

    // Signal terminal completion to trigger final UI reconcile
    let _ = app.emit("scan-finished", "Batch organization cycle completed safely.");

    Ok(format!("Successfully categorized and realigned {} workspace files.", total_files))
}

pub fn execute_and_store_scan(root_path: &str, db_path: &str, excluded_folders: Vec<String>) -> Result<Vec<ScanResult>, String> {
    // 1. Initialize the Database Manager
    let db = DbManager::init(db_path).map_err(|e| format!("DB Init Error: {}", e))?;
    
    // 2. Execute the high-speed 3-stage BLAKE3 scan
    let scan_results = run_dedup_scan(root_path);
    
    // 3. Stream results into SQLite state management with an atomic transaction
    let mut filtered_results = Vec::new();
    
    if let Ok(mut conn_lock) = db.conn.lock() {
        let tx = conn_lock.transaction().map_err(|e| format!("Transaction Start Failure: {}", e))?;
        
        for file in scan_results {
            // Filter out excluded folders
            let is_excluded = excluded_folders.iter().any(|ex| file.file_path.contains(ex));
            if is_excluded {
                continue;
            }

            // Map crypto ScanResult to database structure
            let is_nas = file.file_path.contains("volume1") || file.file_path.contains("/Volumes/");

            // Evict stale AI classification for this path if the file has been modified
            // since the last indexing run (prevents ghost scan metadata persistence).
            tx.execute(
                "DELETE FROM ai_metadata WHERE file_id IN (
                    SELECT id FROM file_index WHERE file_path = ?1 AND modified_at != ?2
                )",
                rusqlite::params![file.file_path, file.modified_at as i64],
            ).map_err(|e| format!("Cache eviction failed for {}: {}", file.file_path, e))?;

            tx.execute(
                "INSERT INTO file_index (
                    file_path, file_name, file_size, modified_at, partial_hash, full_hash, is_nas_path
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(file_path) DO UPDATE SET
                    file_name=excluded.file_name,
                    file_size=excluded.file_size,
                    modified_at=excluded.modified_at,
                    partial_hash=excluded.partial_hash,
                    full_hash=excluded.full_hash,
                    is_nas_path=excluded.is_nas_path",
                rusqlite::params![
                    file.file_path,
                    file.file_name,
                    file.file_size as i64,
                    file.modified_at as i64,
                    file.partial_hash,
                    file.full_hash,
                    if is_nas { 1 } else { 0 }
                ],
            ).map_err(|e| format!("Upsert failed for {}: {}", file.file_path, e))?;

            filtered_results.push(file);
        }
        
        tx.commit().map_err(|e| format!("Transaction Commit Failure: {}", e))?;
    } else {
        return Err("Database Mutex Contention: Failed to acquire write lock.".to_string());
    }
    
    Ok(filtered_results)
}

pub async fn run_ai_classification(
    file_path: &str,
    db_path: &str,
    ollama_url: &str,
    ollama_model: &str,
    gemini_api_key: &str,
    context: IngestionContext,
) -> Result<AiClassificationResult, String> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err("Target system path does not exist on disk layers.".to_string());
    }

    // 0. Caching Skip Guard — only hits if ai_processed_at >= file modified_at,
    //    ensuring a re-scanned file at the same path always forces re-classification.
    let db = DbManager::init(db_path).map_err(|e| format!("DB Access Fault: {}", e))?;
    if let Ok(conn_lock) = db.conn.lock() {
        let cached_result: Option<(String, String, String, f32, bool, Option<String>)> = conn_lock.query_row(
            "SELECT am.suggested_subfolder, am.category, am.correspondent, am.confidence_score, am.tax_relevant, am.identified_member
             FROM ai_metadata am
             JOIN file_index fi ON fi.id = am.file_id
             WHERE fi.file_path = ?1
               AND am.suggested_subfolder IS NOT NULL
               AND am.ai_processed_at IS NOT NULL
               AND am.ai_processed_at >= fi.modified_at",
            rusqlite::params![file_path],
            |row| Ok((row.get(0)?, row.get(1).unwrap_or_default(), row.get(2).unwrap_or_default(), row.get(3)?, row.get::<_, i32>(4)? != 0, row.get(5)?))
        ).ok();

        if let Some((subfolder, category, correspondent, confidence, tax_rel, member)) = cached_result {
            return Ok(AiClassificationResult {
                suggested_subfolder: subfolder,
                category,
                correspondent,
                new_clean_name: path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string(),
                confidence_score: confidence,
                reasoning: "Active cache hit: Skipping inference".to_string(),
                tax_relevant: tax_rel,
                identified_member: member,
            });
        }
    }

    // 1. Calculate file metrics for metadata payload tracking
    let file_name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown_file")
        .to_string();
    
    let file_size = path.metadata()
        .map(|m| m.len())
        .unwrap_or(0);
    let file_size_formatted = format!("{:.2} KB", (file_size as f64) / 1024.0);

    // 2. Extract safe text snippet via our Module 2B native extractor stream
    // Apply head-only parsing logic for OTHERS context
    let mut snippet = extractor::extract_file_snippet(file_path);
    if context == IngestionContext::Others && snippet.len() > 2000 {
        snippet.truncate(2000); // Strict head-only limit for RAD/Medical
    }

    if snippet.is_empty() || snippet.starts_with("Error:") || snippet.starts_with("[SYSTEM ERROR]:") {
        return Err("No valid document text found for classification.".to_string());
    }

    // 3. Assemble data transfer package
    let payload = FileMetadataPayload {
        file_name,
        file_size_formatted,
        textual_snippet: snippet.clone(),
    };

    // 4. Dispatch payload to dual-engine AI pipeline
    let mut ai_result = request_file_classification(payload, ollama_url, ollama_model, gemini_api_key).await?;

    // Override for OTHERS context to route to specialized NAS folders
    if context == IngestionContext::Others {
        ai_result.suggested_subfolder = "Medical_RAD_Ingestion".to_string();
        ai_result.category = "Medical".to_string();
        ai_result.tax_relevant = false;
    }

    // 5. Safely register classification result to permanent SQLite store
    let db = DbManager::init(db_path).map_err(|e| format!("DB Access Fault: {}", e))?;
    
    // Fetch internal file entry ID mapping from path string and insert metadata
    if let Ok(conn_lock) = db.conn.lock() {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let file_id: Option<i64> = conn_lock.query_row(
            "SELECT id FROM file_index WHERE file_path = ?1",
            rusqlite::params![file_path],
            |row| row.get(0)
        ).ok();

        if let Some(id) = file_id {
            let mut conn = conn_lock;
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT OR REPLACE INTO ai_metadata (file_id, extracted_text, suggested_subfolder, category, correspondent, confidence_score, tax_relevant, identified_member, ai_processed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    id,
                    snippet,
                    ai_result.suggested_subfolder,
                    ai_result.category,
                    ai_result.correspondent,
                    ai_result.confidence_score,
                    if ai_result.tax_relevant { 1 } else { 0 },
                    ai_result.identified_member,
                    current_time
                ],
            ).map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
        }
    }

    Ok(ai_result)
}

pub async fn process_single_dropped_file<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    file_path: &str,
    db_path: &str,
    context: IngestionContext,
) -> Result<String, String> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err("Target system path does not exist on disk layers.".to_string());
    }

    if path.is_dir() {
        return Err("Direct directory processing not supported for drop-zone. Please drop files.".to_string());
    }

    let config_dir = app.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let settings_mgr = SettingsManager::new(&config_dir);
    let (ollama_url, ollama_model, gemini_key) = {
        let settings = settings_mgr.current.read().unwrap();
        (
            settings.ollama_base_url.clone(),
            settings.ollama_model.clone(),
            settings.gemini_api_key.clone(),
        )
    };
    let output_base_str = settings_mgr.papers_output_base();
    let output_base = std::path::Path::new(&output_base_str).to_path_buf();

    // 0.5. Wait for the scanner to finish writing before reading any metadata.
    //      The Brother scanner writes via SMB; file size must stabilise before we hash/index.
    {
        let mut prev_size: Option<u64> = None;
        let mut stable = false;
        for _ in 0..10u32 {
            match tokio::fs::metadata(file_path).await {
                Ok(m) if m.len() > 0 => {
                    let sz = m.len();
                    if prev_size == Some(sz) {
                        stable = true;
                        break;
                    }
                    prev_size = Some(sz);
                }
                _ => break,
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        }
        if !stable {
            return Err("File transfer incomplete: size is still changing over SMB. Retry after the scan finishes writing.".to_string());
        }
    }

    // 1. Bypass heavy Stage 1 indexing and perform a minimal 'Stage 0.5' upsert
    // This allows run_ai_classification to track the metadata correctly.
    let db = DbManager::init(db_path).map_err(|e| e.to_string())?;
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
    let file_size = path.metadata().map(|m| m.len()).unwrap_or(0);
    let modified_at = path.metadata().and_then(|m| m.modified()).ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let record = FileRecord {
        id: None,
        file_path: file_path.to_string(),
        file_name: file_name.clone(),
        file_size: file_size as i64,
        modified_at,
        partial_hash: None,
        full_hash: None,
        is_nas_path: file_path.contains("volume1") || file_path.contains("/Volumes/"),
    };
    let _ = db.upsert_file(&record);

    // 2. Trigger immediate classification
    let ai_result = run_ai_classification(file_path, db_path, &ollama_url, &ollama_model, &gemini_key, context).await?;

    // 3. Automated routing if confidence meets the threshold
    let date_str = format!("{}-01-01", current_year_string());
    if ai_result.confidence_score >= 0.5 {
        let combined_category = if ai_result.category.is_empty() {
            ai_result.suggested_subfolder.clone()
        } else {
            format!("{}/{}", ai_result.category, ai_result.suggested_subfolder)
        };
        let target_tree = build_rational_path(&output_base, &combined_category, &date_str);
        let manifest = IngestionManifest {
            original_path: file_path.to_string(),
            suggested_name: ai_result.new_clean_name,
            identified_category: combined_category,
            detected_date: date_str,
            suggested_target_tree: target_tree.to_string_lossy().to_string(),
            is_tax_relevant: ai_result.tax_relevant,
            identified_member: ai_result.identified_member,
            is_unsorted: false,
        };
        serde_json::to_string(&manifest).map_err(|e| format!("Serialization error: {}", e))
    } else {
        let unsorted_dir = unsorted_path(&output_base);
        let manifest = IngestionManifest {
            original_path: file_path.to_string(),
            suggested_name: file_name,
            identified_category: "_Unsorted".to_string(),
            detected_date: date_str,
            suggested_target_tree: unsorted_dir.to_string_lossy().to_string(),
            is_tax_relevant: false,
            identified_member: None,
            is_unsorted: true,
        };
        serde_json::to_string(&manifest).map_err(|e| format!("Serialization error: {}", e))
    }
}


pub async fn execute_relocation_commit<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    batch_id: String,
    original_path: String,
    suggested_name: String,
    identified_category: String,
    detected_date: String,
    storage_tier: String,
    is_tax_relevant: bool,
    db_path: String,
) -> Result<String, String> {
    let config_dir = app.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let settings_mgr = SettingsManager::new(&config_dir);
    let protected_paths = {
        let settings = settings_mgr.current.read().unwrap();
        settings.preset_paths.values().cloned().collect::<Vec<String>>()
    };

    let fs_engine = SafeFileSystemEngine::new(&db_path, protected_paths);
    let original_path_obj = Path::new(&original_path);
    
    let master_containment = if storage_tier == "NAS" {
        let nas_base = settings_mgr.papers_output_base();
        std::path::PathBuf::from(nas_base)
    } else {
        let parent_dir = original_path_obj.parent().unwrap_or_else(|| Path::new("."));
        parent_dir.join("Tfatleek_Output")
    };

    let target_dir = if identified_category == "_Unsorted" {
        unsorted_path(&master_containment)
    } else {
        build_rational_path(&master_containment, &identified_category, &detected_date)
    };
    
    if let Err(e) = std::fs::create_dir_all(&target_dir) {
        return Err(format!("Failed to create containment directory hierarchy: {}", e));
    }
    
    let final_destination_path = target_dir.join(&suggested_name);
    
    fs_engine.execute_safe_move(&batch_id, &original_path, &final_destination_path.to_string_lossy())?;

    // Dual-Routing Trigger: If flagged as tax-relevant (Steuererklaerung), make a background clone
    if is_tax_relevant && identified_category != "_Unsorted" {
        let current_year = &detected_date[0..4];
        let tax_target_dir = master_containment.join("Steuererklaerung").join(current_year);
        std::fs::create_dir_all(&tax_target_dir).map_err(|e| e.to_string())?;
        
        let tax_clone_path = tax_target_dir.join(&suggested_name);
        // Execute the mirror clone operation cleanly
        std::fs::copy(&final_destination_path, &tax_clone_path).map_err(|e| e.to_string())?;
    }

    // Phase D: Remote Vaulting Reconciliation
    let db = DbManager::init(&db_path).map_err(|e| e.to_string())?;
    let file_hash: String = if let Ok(conn_lock) = db.conn.lock() {
        conn_lock.query_row(
            "SELECT full_hash FROM file_index WHERE file_path = ?1",
            rusqlite::params![final_destination_path.to_string_lossy()],
            |row| row.get(0)
        ).unwrap_or_else(|_| "".to_string())
    } else {
        "".to_string()
    };
    
    let _ = reconcile_and_prune_source(&db_path, &file_hash, &original_path).await;
    
    Ok(format!("Successfully routed to {}", final_destination_path.to_string_lossy()))
}

/// Phase D: Remote Vaulting Reconciliation & NAS Cleanup
/// Verifies safety guardrails and transaction status before pruning source files.
pub async fn reconcile_and_prune_source(
    db_path: &str,
    file_hash: &str,
    local_source_path: &str,
) -> Result<(), String> {
    let path = std::path::Path::new(local_source_path);
    let target_lower = local_source_path.to_lowercase();
    
    // Check 1 (Guardrails): Verify root restrictions and protected directories
    if target_lower == "/users/taregahmed/desktop" || 
       target_lower == "/users/taregahmed/documents" || 
       target_lower == "/" {
        return Err("Safety Lock: Reconciliation attempted on protected system root or user profile directory.".to_string());
    }

    // Check 2 (Verification): Ensure the transaction ledger status is COMMITTED
    let db = DbManager::init(db_path).map_err(|e| e.to_string())?;
    let mut is_verified = false;

    if let Ok(conn_lock) = db.conn.lock() {
        // Option A: Check by explicit source path match in committed transactions
        let status: Option<String> = conn_lock.query_row(
            "SELECT status FROM file_transactions WHERE source_path = ?1 AND status = 'COMMITTED' LIMIT 1",
            rusqlite::params![local_source_path],
            |row| row.get(0)
        ).ok();

        if status.is_some() {
            is_verified = true;
        } 
        
        // Option B: Check by file hash if provided (cross-reference with file_index)
        if !is_verified && !file_hash.is_empty() {
            let hash_status: Option<String> = conn_lock.query_row(
                "SELECT ft.status FROM file_transactions ft 
                 JOIN file_index fi ON (ft.source_path = fi.file_path OR ft.destination_path = fi.file_path)
                 WHERE fi.full_hash = ?1 AND ft.status = 'COMMITTED' LIMIT 1",
                rusqlite::params![file_hash],
                |row| row.get(0)
            ).ok();
            if hash_status.is_some() {
                is_verified = true;
            }
        }
    }

    if !is_verified {
        return Err(format!("Reconciliation Failed: No COMMITTED transaction or Paperless success verified for {}", local_source_path));
    }

    // Safety Execution: Prune the local source file if it still exists
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| format!("OS Pruning Failure: {}", e))?;
    }

    Ok(())
}

pub async fn query_contextual_memory_match<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>, incoming_path: String) -> Result<Option<String>, String> {
    let target_lower = incoming_path.to_lowercase();
    if target_lower == "/users/taregahmed/desktop" || target_lower == "/users/taregahmed/documents" || target_lower == "/" {
        return Err("Error: Direct root directory targeting is restricted for system safety. Please target a specific subfolder.".to_string());
    }

    let app_local_data = app_handle.path().app_local_data_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let db_path = app_local_data.join("tfatleek_state.db");
    
    let path_ref = std::path::Path::new(&incoming_path);
    let current_extension = path_ref.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    // Enforce our non-negotiable White-list target guardrail instantly
    let approved_white_list = ["pdf", "docx", "doc", "txt", "log", "dcm", "dicom"];
    if !approved_white_list.contains(&current_extension.as_str()) {
        return Ok(None); // Drop system clutter quietly
    }

    // Connect to the SQLite Memory Graph to check historical layout clusters
    let db = DbManager::init(&db_path.to_string_lossy()).map_err(|e| e.to_string())?;
    if let Ok(conn_lock) = db.conn.lock() {
        // Exact filename match — fuzzy prefix matching caused cross-contamination
        // between consecutive scanner outputs sharing a common prefix (e.g. "Scan_").
        let mut stmt = conn_lock.prepare(
            "SELECT last_known_path FROM file_knowledge_graph WHERE file_name = ?1 LIMIT 1"
        ).map_err(|e| e.to_string())?;

        let file_name_full = path_ref.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let mut rows = stmt.query_map([file_name_full], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
        if let Some(Ok(historical_suggested_path)) = rows.next() {
            return Ok(Some(historical_suggested_path));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests;
