use serde_json;
use core_engine;
use tauri::Manager;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ManifestEntry {
    pub timestamp: String,
    pub filename: String,
    pub sha256: String,
    pub category: String,
    pub subfolder: String,
    pub correspondent: String,
    pub tax_relevant: bool,
    pub identified_member: String,
    pub confidence_score: f64,
    pub new_clean_name: String,
    pub ai_engine: String,
    #[serde(default)]
    pub source_path: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct VerifyResult {
    pub filename: String,
    pub expected_hash: String,
    pub actual_hash: String,
    pub matches: bool,
    pub file_exists: bool,
}

fn collect_manifest_files(dir: &std::path::Path, results: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_manifest_files(&path, results);
        } else if path.file_name().and_then(|n| n.to_str()) == Some("manifest.jsonl") {
            if let Some(s) = path.to_str() {
                results.push(s.to_string());
            }
        }
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[tauri::command]
fn list_manifest_files(nas_root: String) -> Result<Vec<String>, String> {
    let root = std::path::Path::new(&nas_root);
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut results = Vec::new();
    collect_manifest_files(root, &mut results);
    results.sort();
    Ok(results)
}

#[tauri::command]
fn read_manifest(manifest_path: String) -> Result<Vec<ManifestEntry>, String> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(&manifest_path)
        .map_err(|e| format!("Failed to open manifest: {}", e))?;
    let reader = BufReader::new(file);
    let mut entries: Vec<ManifestEntry> = Vec::new();
    for line in reader.lines().flatten() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<ManifestEntry>(&line) {
            Ok(mut entry) => {
                entry.source_path = manifest_path.clone();
                entries.push(entry);
            }
            Err(e) => {
                eprintln!("Warning: skipping malformed manifest line: {}", e);
            }
        }
    }
    entries.reverse();
    Ok(entries)
}

#[tauri::command]
fn verify_manifest_entry(
    manifest_path: String,
    filename: String,
    expected_hash: String,
) -> Result<VerifyResult, String> {
    use sha2::{Sha256, Digest};
    use std::io::{Read, BufReader};

    let parent = std::path::Path::new(&manifest_path)
        .parent()
        .ok_or_else(|| "Invalid manifest path".to_string())?;
    let file_path = parent.join(&filename);
    let file_exists = file_path.exists();

    if !file_exists {
        return Ok(VerifyResult {
            filename,
            expected_hash,
            actual_hash: String::new(),
            matches: false,
            file_exists: false,
        });
    }

    let file = std::fs::File::open(&file_path)
        .map_err(|e| format!("Failed to open file: {}", e))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let actual_hash = format!("{:x}", hasher.finalize());
    let matches = actual_hash.to_lowercase() == expected_hash.to_lowercase();

    Ok(VerifyResult {
        filename,
        expected_hash,
        actual_hash,
        matches,
        file_exists: true,
    })
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct PendingFile {
    pub filename: String,
    pub size_bytes: u64,
    pub modified_at: u64,
}

#[tauri::command]
fn get_pending_queue_status(pending_path: Option<String>) -> Result<Vec<PendingFile>, String> {
    let path_str = pending_path.unwrap_or_else(|| "/Volumes/Papers/Tfatleek_Inbox/_pending/".to_string());
    
    // Safety check for absolute path guards
    let path_lower = path_str.to_lowercase();
    if path_lower == "/" || 
       path_lower == "/users/taregahmed/desktop" || path_lower == "/users/taregahmed/documents" ||
       path_lower == "/users/taregshek/desktop" || path_lower == "/users/taregshek/documents" {
        return Err("Error: Direct root directory targeting is restricted for system safety.".to_string());
    }

    let dir_path = std::path::Path::new(&path_str);
    if !dir_path.exists() || !dir_path.is_dir() {
        return Ok(vec![]);
    }

    let entries = match std::fs::read_dir(dir_path) {
        Ok(read_dir) => read_dir,
        Err(_) => return Ok(vec![]),
    };

    let mut pending_files = Vec::new();
    for entry_result in entries {
        if let Ok(entry) = entry_result {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            
            if file_type.is_file() {
                let filename = entry.file_name().to_string_lossy().into_owned();
                
                // Skip hidden files
                if filename.starts_with('.') {
                    continue;
                }

                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let size_bytes = metadata.len();
                let modified_at = metadata.modified()
                    .map(|t| t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))
                    .unwrap_or(0);

                pending_files.push(PendingFile {
                    filename,
                    size_bytes,
                    modified_at,
                });
            }
        }
    }

    pending_files.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(pending_files)
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct WatcherHealth {
    pub status: String,
    pub timestamp: String,
    pub seconds_ago: i64,
    pub pid: Option<i64>,
    pub file_exists: bool,
    pub interval_seconds: Option<i64>,
}

#[tauri::command]
fn get_watcher_health(heartbeat_path: String) -> Result<WatcherHealth, String> {
    use chrono::{DateTime, Utc};

    let path = std::path::Path::new(&heartbeat_path);
    if !path.exists() {
        return Ok(WatcherHealth {
            status: "unreachable".to_string(),
            timestamp: String::new(),
            seconds_ago: -1,
            pid: None,
            file_exists: false,
            interval_seconds: None,
        });
    }

    let data = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read heartbeat: {}", e))?;
    let heartbeat: serde_json::Value = serde_json::from_str(&data)
        .map_err(|e| format!("Failed to parse heartbeat: {}", e))?;

    let timestamp_str = heartbeat["timestamp"].as_str().unwrap_or("").to_string();
    let pid = heartbeat["pid"].as_i64();
    let interval_seconds = heartbeat["interval_seconds"].as_i64();

    let seconds_ago = if !timestamp_str.is_empty() {
        match DateTime::parse_from_rfc3339(&timestamp_str) {
            Ok(dt) => {
                let now = Utc::now();
                let hb_utc: DateTime<Utc> = dt.into();
                (now - hb_utc).num_seconds()
            }
            Err(_) => -1,
        }
    } else {
        -1
    };

    let status = if seconds_ago < 0 {
        "unreachable".to_string()
    } else if seconds_ago < 120 {
        "alive".to_string()
    } else if seconds_ago < 600 {
        "stale".to_string()
    } else {
        "unreachable".to_string()
    };

    Ok(WatcherHealth {
        status,
        timestamp: timestamp_str,
        seconds_ago,
        pid,
        file_exists: true,
        interval_seconds,
    })
}

#[tauri::command]
fn get_watcher_log_tail(log_path: String, lines: usize) -> Result<Vec<String>, String> {
    use std::io::{Read, Seek, SeekFrom};

    let path = std::path::Path::new(&log_path);
    if !path.exists() {
        return Ok(vec![]);
    }

    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open log: {}", e))?;
    let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let read_size: u64 = 32768u64.min(file_size);

    if read_size == 0 {
        return Ok(vec![]);
    }

    file.seek(SeekFrom::End(-(read_size as i64)))
        .map_err(|e| e.to_string())?;

    let mut buf = Vec::with_capacity(read_size as usize);
    file.read_to_end(&mut buf).map_err(|e| e.to_string())?;

    let content = String::from_utf8_lossy(&buf);
    let all_lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let start = all_lines.len().saturating_sub(lines);
    Ok(all_lines[start..].to_vec())
}

#[tauri::command]
fn export_manifest_csv(manifest_path: String) -> Result<String, String> {
    let entries = read_manifest(manifest_path)?;
    let mut csv = String::from(
        "timestamp,filename,sha256,category,subfolder,correspondent,tax_relevant,identified_member,confidence_score,new_clean_name,ai_engine\n",
    );
    for e in entries {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{:.4},{},{}\n",
            csv_escape(&e.timestamp),
            csv_escape(&e.filename),
            csv_escape(&e.sha256),
            csv_escape(&e.category),
            csv_escape(&e.subfolder),
            csv_escape(&e.correspondent),
            e.tax_relevant,
            csv_escape(&e.identified_member),
            e.confidence_score,
            csv_escape(&e.new_clean_name),
            csv_escape(&e.ai_engine),
        ));
    }
    Ok(csv)
}

fn get_db_path(handle: &tauri::AppHandle) -> String {
    let app_local_data = handle.path().app_local_data_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&app_local_data).ok();
    let db_path = app_local_data.join("tfatleek_state.db");
    db_path.to_string_lossy().to_string()
}

#[tauri::command]
async fn start_dedup_scan(handle: tauri::AppHandle, target_path: String) -> Result<String, String> {
    let target_lower = target_path.to_lowercase();
    if target_lower == "/users/taregahmed/desktop" || target_lower == "/users/taregahmed/documents" ||
       target_lower == "/users/taregshek/desktop" || target_lower == "/users/taregshek/documents" {
        return Err("Error: Direct root directory targeting is restricted for system safety. Please target a specific subfolder.".to_string());
    }
    let db_path = get_db_path(&handle);
    
    let config_dir = handle.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let settings_mgr = core_engine::settings::SettingsManager::new(&config_dir);
    let excluded_folders = {
        let settings = settings_mgr.current.read().unwrap();
        settings.excluded_folders.iter().cloned().collect::<Vec<String>>()
    };

    // Dispatch execution to a background thread to keep UI interaction at 60fps
    tokio::task::spawn_blocking(move || {
        match core_engine::execute_and_store_scan(&target_path, &db_path, excluded_folders) {
            Ok(results) => {
                // Return data payload serialized to JSON for React consumption
                serde_json::to_string(&results).map_err(|e| e.to_string())
            },
            Err(e) => Err(e),
        }
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn classify_file_with_ai(handle: tauri::AppHandle, file_path: String) -> Result<String, String> {
    let db_path = get_db_path(&handle);
    let config_dir = handle.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let settings_mgr = core_engine::settings::SettingsManager::new(&config_dir);
    let (ollama_url, ollama_model, gemini_key) = {
        let settings = settings_mgr.current.read().unwrap();
        (
            settings.ollama_base_url.clone(),
            settings.ollama_model.clone(),
            settings.gemini_api_key.clone(),
        )
    };

    match core_engine::run_ai_classification(&file_path, &db_path, &ollama_url, &ollama_model, &gemini_key, core_engine::IngestionContext::Private).await {
        Ok(result) => {
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }
        Err(e) => Err(format!("AI Orchestration Engine Abort: {}", e)),
    }
}

#[tauri::command]
async fn trigger_system_undo(handle: tauri::AppHandle) -> Result<String, String> {
    let db_path = get_db_path(&handle);
    let config_dir = handle.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let settings_mgr = core_engine::settings::SettingsManager::new(&config_dir);
    let protected_paths = {
        let settings = settings_mgr.current.read().unwrap();
        settings.preset_paths.values().cloned().collect::<Vec<String>>()
    };
    
    let fs_engine = core_engine::transactions::SafeFileSystemEngine::new(&db_path, protected_paths);
    fs_engine.execute_undo_last_transaction()
}

#[tauri::command]
async fn undo_last_batch(handle: tauri::AppHandle, batch_id: String) -> Result<(), String> {
    let db_path = get_db_path(&handle);
    let config_dir = handle.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let settings_mgr = core_engine::settings::SettingsManager::new(&config_dir);
    let protected_paths = {
        let settings = settings_mgr.current.read().unwrap();
        settings.preset_paths.values().cloned().collect::<Vec<String>>()
    };
    
    let fs_engine = core_engine::transactions::SafeFileSystemEngine::new(&db_path, protected_paths);
    fs_engine.execute_system_undo(batch_id).await
}

#[tauri::command]
async fn trigger_batch_ai_organization(
    app_handle: tauri::AppHandle, 
    target_path: String,
    _custom_excludes: Vec<String>
) -> Result<String, String> {
    let target_lower = target_path.to_lowercase();
    if target_lower == "/users/taregahmed/desktop" || target_lower == "/users/taregahmed/documents" ||
       target_lower == "/users/taregshek/desktop" || target_lower == "/users/taregshek/documents" ||
       target_lower == "/" {
        return Err("Error: Direct root directory targeting is restricted for system safety. Please target a specific subfolder.".to_string());
    }

    let app_local_data = app_handle.path().app_local_data_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let db_path = app_local_data.join("tfatleek_state.db");
    let db_path_str = db_path.to_string_lossy();

    core_engine::execute_batch_organization(app_handle, &target_path, &db_path_str).await
}

#[tauri::command]
async fn fetch_isolated_duplicates(handle: tauri::AppHandle) -> Result<String, String> {
    let db_path = get_db_path(&handle);
    let db = database::DbManager::init(&db_path).map_err(|e| e.to_string())?;
    let duplicate_groups = db.find_duplicates().map_err(|e| e.to_string())?;
    
    serde_json::to_string(&duplicate_groups).map_err(|e| e.to_string())
}

#[tauri::command]
async fn execute_file_deletion(handle: tauri::AppHandle, file_path: String) -> Result<String, String> {
    let target_lower = file_path.to_lowercase();
    if target_lower == "/users/taregahmed/desktop" || target_lower == "/users/taregahmed/documents" ||
       target_lower == "/users/taregshek/desktop" || target_lower == "/users/taregshek/documents" {
        return Err("Error: Direct root directory targeting is restricted for system safety. Please target a specific subfolder.".to_string());
    }
    
    let config_dir = handle.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let settings_mgr = core_engine::settings::SettingsManager::new(&config_dir);
    if settings_mgr.is_path_protected(&file_path) {
        return Err(format!("Safety Lock: File deletion forbidden on protected path: {}", file_path));
    }

    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err("Target file does not exist on storage arrays.".to_string());
    }
    
    std::fs::remove_file(path)
        .map_err(|e| format!("OS secure file removal failure: {}", e))?;
        
    Ok(format!("File successfully unlinked from volume path: {}", file_path))
}

#[tauri::command]
async fn get_settings(handle: tauri::AppHandle) -> Result<core_engine::settings::AppSettings, String> {
    let config_dir = handle.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let settings_mgr = core_engine::settings::SettingsManager::new(&config_dir);
    let settings = settings_mgr.current.read().map_err(|e| e.to_string())?;
    Ok(settings.clone())
}

#[tauri::command]
async fn add_preset_path(handle: tauri::AppHandle, name: String, path: String) -> Result<(), String> {
    let config_dir = handle.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let settings_mgr = core_engine::settings::SettingsManager::new(&config_dir);
    settings_mgr.set_preset_path(name, path)
}

#[tauri::command]
async fn add_excluded_folder(handle: tauri::AppHandle, folder: String) -> Result<(), String> {
    let config_dir = handle.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let settings_mgr = core_engine::settings::SettingsManager::new(&config_dir);
    settings_mgr.add_excluded_folder(folder)
}

#[tauri::command]
async fn remove_excluded_folder(handle: tauri::AppHandle, folder: String) -> Result<(), String> {
    let config_dir = handle.path().app_config_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let settings_mgr = core_engine::settings::SettingsManager::new(&config_dir);
    settings_mgr.remove_excluded_folder(&folder)
}

#[tauri::command]
async fn index_master_tree(target_path: String) -> Result<Vec<String>, String> {
    let indexer = core_engine::settings::MasterTreeIndexer::new();
    indexer.index_path(&target_path)?;
    Ok(indexer.get_cached_structure(&target_path).unwrap_or_default())
}

#[tauri::command]
async fn process_single_dropped_file(handle: tauri::AppHandle, path: String, context: core_engine::IngestionContext) -> Result<String, String> {
    let db_path = get_db_path(&handle);
    core_engine::process_single_dropped_file(handle, &path, &db_path, context).await
}

#[tauri::command]
async fn execute_relocation_commit(
    handle: tauri::AppHandle,
    batch_id: String,
    original_path: String,
    suggested_name: String,
    identified_category: String,
    detected_date: String,
    storage_tier: String,
    is_tax_relevant: bool,
) -> Result<String, String> {
    let db_path = get_db_path(&handle);
    core_engine::execute_relocation_commit(
        handle,
        batch_id,
        original_path,
        suggested_name,
        identified_category,
        detected_date,
        storage_tier,
        is_tax_relevant,
        db_path,
    ).await
}

#[tauri::command]
async fn query_contextual_memory_match(handle: tauri::AppHandle, incoming_path: String) -> Result<Option<String>, String> {
    core_engine::query_contextual_memory_match(handle, incoming_path).await
}

#[tauri::command]
fn get_family_presets() -> Vec<core_engine::FamilyMember> {
    core_engine::get_default_family_presets()
}

#[tauri::command]
async fn submit_to_paperless_vault(
    handle: tauri::AppHandle,
    file_path: String,
) -> Result<core_engine::paperless_bridge::BridgeResponse, String> {
    let db_path = get_db_path(&handle);
    
    // 1. Dispatch upload to Paperless-ngx vault
    let resp = core_engine::paperless_bridge::submit_to_paperless_vault(file_path.clone()).await?;
    
    // 2. Post-Ingestion Reconciliation (Phase D)
    if resp.status == "SUCCESS" {
        let db = database::DbManager::init(&db_path).map_err(|e| e.to_string())?;
        
        // Register a virtual transaction to satisfy safety ledger verification
        if let Ok(conn_lock) = db.conn.lock() {
            let _ = conn_lock.execute(
                "INSERT INTO file_transactions (batch_id, operation_type, source_path, destination_path, status) 
                 VALUES (?1, 'VAULT', ?2, 'PAPERLESS_NGX', 'COMMITTED')",
                rusqlite::params!["paperless_ingestion", file_path],
            );
        }

        // Fetch file hash for multi-factor reconciliation verification
        let file_hash: String = if let Ok(conn_lock) = db.conn.lock() {
            conn_lock.query_row(
                "SELECT full_hash FROM file_index WHERE file_path = ?1",
                rusqlite::params![file_path],
                |row| row.get(0)
            ).unwrap_or_else(|_| "".to_string())
        } else {
            "".to_string()
        };

        // Trigger the safe reconciliation & pruning layer
        core_engine::reconcile_and_prune_source(&db_path, &file_hash, &file_path).await?;
    }
    
    Ok(resp)
}

#[tauri::command]
async fn get_smart_groups(handle: tauri::AppHandle) -> Result<Vec<database::SmartGroup>, String> {
    let db_path = get_db_path(&handle);
    let db = database::DbManager::init(&db_path).map_err(|e| e.to_string())?;
    db.get_smart_groups().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_smart_group_files(
    handle: tauri::AppHandle,
    group_id: i64,
) -> Result<Vec<database::FileIndexEntry>, String> {
    let db_path = get_db_path(&handle);
    let db = database::DbManager::init(&db_path).map_err(|e| e.to_string())?;
    db.get_smart_group_files(group_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_related_documents(
    handle: tauri::AppHandle,
    file_id: String,
) -> Result<Vec<database::FileIndexEntry>, String> {
    let db_path = get_db_path(&handle);
    let db = database::DbManager::init(&db_path).map_err(|e| e.to_string())?;
    db.get_related_documents(&file_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_smart_group(
    handle: tauri::AppHandle,
    group: database::NewSmartGroup,
) -> Result<database::SmartGroup, String> {
    let db_path = get_db_path(&handle);
    let db = database::DbManager::init(&db_path).map_err(|e| e.to_string())?;
    db.create_smart_group(group).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_smart_group(handle: tauri::AppHandle, group_id: i64) -> Result<(), String> {
    if group_id <= 4 {
        return Err("Cannot delete built-in groups.".to_string());
    }
    let db_path = get_db_path(&handle);
    let db = database::DbManager::init(&db_path).map_err(|e| e.to_string())?;
    db.delete_smart_group(group_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_file(path: String) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to open file: {}", e))?;
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct N8nExecutionStatus {
    pub id: String,
    pub status: String,
    pub started_at: String,
    pub stopped_at: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct N8nApiResponse {
    pub data: Vec<N8nExecutionStatus>,
}

fn load_env_file() {
    if let Ok(content) = std::fs::read_to_string(".env") {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, val)) = line.split_once('=') {
                std::env::set_var(key.trim(), val.trim());
            }
        }
        return;
    }
    if let Ok(content) = std::fs::read_to_string("../.env") {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, val)) = line.split_once('=') {
                std::env::set_var(key.trim(), val.trim());
            }
        }
    }
}

#[tauri::command]
async fn get_n8n_workflow_status() -> Result<N8nExecutionStatus, String> {
    load_env_file();

    let api_key = std::env::var("N8N_API_KEY")
        .map_err(|_| "N8N_API_KEY environment variable is not set".to_string())?;

    if api_key.trim().is_empty() {
        return Err("N8N_API_KEY is empty".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let url = "http://192.168.254.18:5678/api/v1/executions?workflowId=ixR5Sr2qS7QdqCsd&limit=1";
    let resp = client.get(url)
        .header("X-N8N-API-KEY", api_key)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("n8n API returned error status: {}", resp.status()));
    }

    let parsed: N8nApiResponse = resp.json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    if let Some(execution) = parsed.data.into_iter().next() {
        Ok(execution)
    } else {
        Err("No execution history found for this workflow".to_string())
    }
}

async fn start_delta_sync_daemon(handle: tauri::AppHandle) {
    use chrono::Utc;
    loop {
        let db_path = get_db_path(&handle);
        load_env_file();

        match database::DbManager::init(&db_path) {
            Ok(db) => {
                let last_sync = db.get_setting("last_paperless_sync_timestamp")
                    .unwrap_or(None)
                    .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

                match core_engine::paperless_bridge::fetch_modified_documents(&last_sync).await {
                    Ok(modified_docs) => {
                        for doc in modified_docs {
                            let _ = db.update_metadata_from_paperless(
                                &doc.file_name,
                                doc.correspondent.clone(),
                                doc.created_date.clone(),
                            );
                        }
                        let now_iso = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                        let _ = db.set_setting("last_paperless_sync_timestamp", &now_iso);
                    }
                    Err(e) => {
                        eprintln!("Delta-Sync Daemon Error: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Delta-Sync Daemon Database Error: {}", e);
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(600)).await;
    }
}

#[tauri::command]
async fn update_paperless_settings(
    handle: tauri::AppHandle,
    nas_ip: String,
    api_token: String,
) -> Result<(), String> {
    let config_dir = handle.path().app_config_dir()
        .map_err(|e| format!("Failed to resolve app config directory: {}", e))?;
    let settings_mgr = core_engine::settings::SettingsManager::new(&config_dir);
    {
        let mut settings = settings_mgr.current.write().map_err(|e| e.to_string())?;
        settings.paperless_nas_ip = nas_ip;
        settings.paperless_api_token = api_token;
    }
    settings_mgr.save()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                start_delta_sync_daemon(handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_dedup_scan,
            classify_file_with_ai,
            trigger_system_undo,
            undo_last_batch,
            trigger_batch_ai_organization,
            fetch_isolated_duplicates,
            execute_file_deletion,
            process_single_dropped_file,
            execute_relocation_commit,
            query_contextual_memory_match,
            get_family_presets,
            get_settings,
            add_preset_path,
            add_excluded_folder,
            remove_excluded_folder,
            index_master_tree,
            submit_to_paperless_vault,
            update_paperless_settings,
            get_smart_groups,
            get_smart_group_files,
            create_smart_group,
            delete_smart_group,
            open_file,
            list_manifest_files,
            read_manifest,
            verify_manifest_entry,
            export_manifest_csv,
            get_watcher_health,
            get_watcher_log_tail,
            get_pending_queue_status,
            get_related_documents,
            get_n8n_workflow_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
