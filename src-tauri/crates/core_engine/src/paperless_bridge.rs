use std::path::PathBuf;
use reqwest::multipart;
use crate::IngestionContext;
use crate::settings::AppSettings;

pub async fn upload_to_paperless(
    file_path: PathBuf,
    context: IngestionContext,
    settings: AppSettings,
    db_path: String,
) -> Result<(), String> {
    if settings.paperless_nas_ip.is_empty() || settings.paperless_api_token.is_empty() {
        return Err("Paperless configuration (IP or Token) is missing in settings.".to_string());
    }

    let url = format!("http://{}:25680/api/documents/post_document/", settings.paperless_nas_ip);
    let client = reqwest::Client::new();

    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid file name")?
        .to_string();

    let file_content = tokio::fs::read(&file_path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;
    
    let file_part = multipart::Part::bytes(file_content)
        .file_name(file_name.clone());

    let mut form = multipart::Form::new()
        .part("document", file_part);

    // Fetch identified member from DB if available
    let identified_member = if let Ok(db) = database::DbManager::init(&db_path) {
        if let Ok(conn_lock) = db.conn.lock() {
            conn_lock.query_row(
                "SELECT identified_member FROM ai_metadata 
                 JOIN file_index ON file_index.id = ai_metadata.file_id 
                 WHERE file_index.file_path = ?1",
                rusqlite::params![file_path.to_string_lossy().to_string()],
                |row| row.get::<_, Option<String>>(0)
            ).unwrap_or(None)
        } else {
            None
        }
    } else {
        None
    };

    // Dynamically inject corresponding headers or tags based on the IngestionContext
    match context {
        IngestionContext::Private => {
            form = form.text("title", file_name);
            let mut tags = vec!["Papers".to_string(), "Family".to_string()];
            if let Some(member) = identified_member {
                tags.push(member);
            }
            form = form.text("tags", tags.join(","));
        }
        IngestionContext::Others => {
            form = form.text("title", format!("[MEDICAL] {}", file_name));
            form = form.text("tags", "Clinical,Radiological,Others");
        }
    }

    let response = client
        .post(url)
        .header("Authorization", format!("Token {}", settings.paperless_api_token))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Network error connecting to Paperless: {}", e))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "Could not read error body".to_string());
        Err(format!("Paperless API error ({}): {}", status, body))
    }
}
