use std::path::PathBuf;
use reqwest::multipart;
use serde::{Serialize, Deserialize};
use tokio_util::io::ReaderStream;
use crate::IngestionContext;
use crate::settings::AppSettings;

#[derive(Serialize, Deserialize)]
pub struct BridgeResponse {
    pub status: String,
    pub document_id: Option<i64>,
}

#[derive(Deserialize)]
struct PaperlessTag {
    id: u32,
    name: String,
}

#[derive(Deserialize)]
struct PaperlessTagsResponse {
    results: Vec<PaperlessTag>,
}

async fn resolve_tag_ids(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
    tag_names: &[String],
) -> Vec<u32> {
    let url = format!("{}/api/tags/?page_size=500", base_url);
    let Ok(resp) = client
        .get(&url)
        .header("Authorization", format!("Token {}", token))
        .send()
        .await
    else {
        return vec![];
    };
    let Ok(data) = resp.json::<PaperlessTagsResponse>().await else {
        return vec![];
    };
    tag_names
        .iter()
        .filter_map(|name| {
            data.results
                .iter()
                .find(|t| t.name.eq_ignore_ascii_case(name))
                .map(|t| t.id)
        })
        .collect()
}

/// Paperless-ngx API bridge — token read from PAPERLESS_TOKEN env var at runtime.
pub async fn submit_to_paperless_vault(file_path: String) -> Result<BridgeResponse, String> {
    let path = PathBuf::from(&file_path);
    if !path.exists() {
        return Err(format!("Vault Error: File not found at {}", file_path));
    }

    let endpoint = "http://192.168.254.18:25680/api/documents/post_document/";
    let auth_token = std::env::var("PAPERLESS_TOKEN")
        .map_err(|_| "PAPERLESS_TOKEN environment variable is not set".to_string())?;
    let inbox_tag_id = "25";

    let file_name = path.file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid file name")?
        .to_string();

    let client = reqwest::Client::new();
    
    // Open file for async streaming to minimize memory footprint during large PDF uploads
    let file = tokio::fs::File::open(&path).await
        .map_err(|e| format!("IO Error: {}", e))?;
    let stream = ReaderStream::new(file);
    let body = reqwest::Body::wrap_stream(stream);
    
    let file_part = multipart::Part::stream(body)
        .file_name(file_name);

    let form = multipart::Form::new()
        .part("document", file_part)
        .text("tags", inbox_tag_id.to_string());

    let response = client
        .post(endpoint)
        .header("Authorization", format!("Token {}", auth_token))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Vault Connection Failure: {}", e))?;

    if response.status().is_success() {
        Ok(BridgeResponse {
            status: "SUCCESS".to_string(),
            document_id: None, // Paperless post_document returns 200/202 with task info, not ID directly
        })
    } else {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_else(|_| "Unknown Error".to_string());
        Err(format!("Paperless API Rejection ({}): {}", status, error_body))
    }
}

pub async fn upload_to_paperless(
    file_path: PathBuf,
    context: IngestionContext,
    settings: AppSettings,
    db_path: String,
) -> Result<(), String> {
    if settings.paperless_nas_ip.is_empty() || settings.paperless_api_token.is_empty() {
        return Err("Paperless configuration (IP or Token) is missing in settings.".to_string());
    }

    let base_url = format!("http://{}:25680", settings.paperless_nas_ip);
    let url = format!("{}/api/documents/post_document/", base_url);
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
            let mut tag_names = vec!["Papers".to_string(), "Family".to_string()];
            if let Some(member) = identified_member {
                tag_names.push(member);
            }
            let tag_ids = resolve_tag_ids(&client, &base_url, &settings.paperless_api_token, &tag_names).await;
            for id in tag_ids {
                form = form.text("tags", id.to_string());
            }
        }
        IngestionContext::Others => {
            form = form.text("title", format!("[MEDICAL] {}", file_name));
            let tag_names = vec!["Clinical".to_string(), "Radiological".to_string(), "Others".to_string()];
            let tag_ids = resolve_tag_ids(&client, &base_url, &settings.paperless_api_token, &tag_names).await;
            for id in tag_ids {
                form = form.text("tags", id.to_string());
            }
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
