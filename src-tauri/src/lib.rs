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
    
    // Dispatch execution to a background thread to keep UI interaction at 60fps
    tokio::task::spawn_blocking(move || {
        match core_engine::execute_and_store_scan(&target_path, &db_path) {
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
    let model_target = "gemma4:e4b"; // Pinning optimized model size for 16GB systems

    match core_engine::run_ai_classification(&file_path, &db_path, model_target).await {
        Ok(result) => {
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }
        Err(e) => Err(format!("AI Orchestration Engine Abort: {}", e)),
    }
}

#[tauri::command]
async fn trigger_system_undo(handle: tauri::AppHandle) -> Result<String, String> {
    let db_path = get_db_path(&handle);
    let fs_engine = core_engine::transactions::SafeFileSystemEngine::new(&db_path);
    fs_engine.execute_undo_last_transaction()
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
    let model_target = "gemma4:e4b";

    core_engine::execute_batch_organization(app_handle, &target_path, &db_path_str, model_target).await
}

#[tauri::command]
async fn fetch_isolated_duplicates(handle: tauri::AppHandle) -> Result<String, String> {
    let db_path = get_db_path(&handle);
    let db = database::DbManager::init(&db_path).map_err(|e| e.to_string())?;
    let duplicate_groups = db.find_duplicates().map_err(|e| e.to_string())?;
    
    serde_json::to_string(&duplicate_groups).map_err(|e| e.to_string())
}

#[tauri::command]
async fn execute_file_deletion(file_path: String) -> Result<String, String> {
    let target_lower = file_path.to_lowercase();
    if target_lower == "/users/taregahmed/desktop" || target_lower == "/users/taregahmed/documents" {
        return Err("Error: Direct root directory targeting is restricted for system safety. Please target a specific subfolder.".to_string());
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
async fn process_single_dropped_file(handle: tauri::AppHandle, path: String) -> Result<String, String> {
    let db_path = get_db_path(&handle);
    let model_target = "gemma4:e4b";
    core_engine::process_single_dropped_file(handle, &path, &db_path, model_target).await
}

#[tauri::command]
async fn execute_relocation_commit(
    handle: tauri::AppHandle,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            start_dedup_scan,
            classify_file_with_ai,
            trigger_system_undo,
            trigger_batch_ai_organization,
            fetch_isolated_duplicates,
            execute_file_deletion,
            process_single_dropped_file,
            execute_relocation_commit,
            query_contextual_memory_match,
            get_family_presets
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
