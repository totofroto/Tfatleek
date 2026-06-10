use serde_json;
use core_engine;
use tauri::Manager;

fn get_db_path(handle: &tauri::AppHandle) -> String {
    let app_local_data = handle.path().app_local_data_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&app_local_data).ok();
    let db_path = app_local_data.join("tfatleek_state.db");
    db_path.to_string_lossy().to_string()
}

#[tauri::command]
async fn start_dedup_scan(handle: tauri::AppHandle, target_path: String) -> Result<String, String> {
    let target_lower = target_path.to_lowercase();
    if target_lower == "/users/taregahmed/desktop" || target_lower == "/users/taregahmed/documents" {
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
    if target_lower == "/users/taregahmed/desktop" || target_lower == "/users/taregahmed/documents" || target_lower == "/" {
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
    if target_lower == "/users/taregahmed/desktop" || target_lower == "/users/taregahmed/documents" {
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
            open_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
