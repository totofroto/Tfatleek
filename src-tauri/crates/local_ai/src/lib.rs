use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadataPayload {
    pub file_name: String,
    pub file_size_formatted: String,
    pub textual_snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiClassificationResult {
    pub suggested_subfolder: String,
    pub new_clean_name: String,
    pub confidence_score: f64,
    pub reasoning: String,
    pub is_tax_relevant: bool,
}

pub async fn request_file_classification(
    payload: FileMetadataPayload,
    target_model: &str,
) -> Result<AiClassificationResult, String> {
    let client = reqwest::Client::new();
    let url = "http://localhost:11434/api/chat";

    // System orchestration limits enforcing exact structural boundaries
    let system_prompt = "You are Tfatleek's backend filing clerk. Analyze the provided file metadata and text snippets. Choose a clean, descriptive snake_case folder name for 'suggested_subfolder'. If the file needs clarification, provide a human-readable clean file name string. Evaluate if the document has high semantic affinity to tax preparation or fiscal reporting (e.g., invoices, bank statements, tax returns) and set 'is_tax_relevant' accordingly. Return your answer strictly within the JSON schema constraint.";

    let user_content = format!(
        "File Name: {}\nSize: {}\nSnippet Content Preview: \n\"\"\"\n{}\n\"\"\"",
        payload.file_name, payload.file_size_formatted, payload.textual_snippet
    );

    // Explicit payload configuration mapping Ollama's native JSON Schema constraints
    let request_body = serde_json::json!({
        "model": target_model, // Targeted baseline maps directly to "gemma4:e4b"
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_content }
        ],
        "stream": false,
        "format": {
            "type": "object",
            "properties": {
                "suggested_subfolder": { "type": "string" },
                "new_clean_name": { "type": "string" },
                "confidence_score": { "type": "number" },
                "is_tax_relevant": { "type": "boolean" },
                "reasoning": { "type": "string" }
            },
            "required": ["suggested_subfolder", "new_clean_name", "confidence_score", "is_tax_relevant", "reasoning"]
        },
        "options": {
            "temperature": 0.0,
            "top_p": 0.1
        },
        "keep_alive": "5m" // Enforces immediate memory release off Apple Silicon unified VRAM
    });

    let response = client
        .post(url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Ollama daemon link timeout or failure: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Local AI model instance returned failure code: {}",
            response.status()
        ));
    }

    #[derive(Deserialize)]
    struct OllamaMessage {
        content: String,
    }
    #[derive(Deserialize)]
    struct OllamaResponseWrapper {
        message: OllamaMessage,
    }

    let outer_json: OllamaResponseWrapper = response
        .json()
        .await
        .map_err(|e| format!("Failed to read raw incoming network object: {}", e))?;

    // Parse the inner schema string directly back into our application-level native struct
    let final_classification: AiClassificationResult = serde_json::from_str(&outer_json.message.content)
        .map_err(|e| {
            format!(
                "Received data broke schema layout contract: {}. Content: {}",
                e, outer_json.message.content
            )
        })?;

    Ok(final_classification)
}

#[cfg(test)]
mod tests {
    // Tests for local AI integration would typically require a running Ollama instance.
    // We can add mock tests if necessary in the future.
}
