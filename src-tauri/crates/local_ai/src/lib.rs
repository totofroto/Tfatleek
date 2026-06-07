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
    pub category: String,
    pub correspondent: String,
    pub new_clean_name: String,
    pub confidence_score: f32,
    pub reasoning: String,
    pub tax_relevant: bool,
    pub identified_member: Option<String>,
}

pub async fn request_file_classification(
    payload: FileMetadataPayload,
    ollama_url: &str,
    ollama_model: &str,
    gemini_api_key: &str,
) -> Result<AiClassificationResult, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let url = if ollama_url.ends_with("/api/chat") {
        ollama_url.to_string()
    } else {
        format!("{}/api/chat", ollama_url.trim_end_matches('/'))
    };

    // System orchestration limits enforcing exact structural boundaries
    let system_prompt = "You are Tfatleek's backend filing clerk. Analyze the provided file metadata and text snippets. \
    Choose a clean, descriptive snake_case folder name for 'suggested_subfolder'. \
    Assign a 'category' (e.g., invoices, medical, bank_statements, personal). \
    Identify the 'correspondent' (organization or person who sent/received the document). \
    If the file needs clarification, provide a human-readable clean file name string in 'new_clean_name'. \
    Evaluate if the document has high semantic affinity to tax preparation or fiscal reporting and set 'tax_relevant' accordingly. \
    Also check if the content mentions any family members: Tareg Mohamed Ahmed Shek (Father), Miluda Bashir Shek (Mother), Fatima Shek (Daughter), or Sama Shek (Daughter). \
    If a clear match is found, return their full name in 'identified_member'. \
    Return your answer strictly within the JSON schema constraint.";

    let user_content = format!(
        "File Name: {}\nSize: {}\nSnippet Content Preview: \n\"\"\"\n{}\n\"\"\"",
        payload.file_name, payload.file_size_formatted, payload.textual_snippet
    );

    // Explicit payload configuration mapping Ollama's native JSON Schema constraints
    let request_body = serde_json::json!({
        "model": ollama_model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_content }
        ],
        "stream": false,
        "format": {
            "type": "object",
            "properties": {
                "suggested_subfolder": { "type": "string" },
                "category": { "type": "string" },
                "correspondent": { "type": "string" },
                "new_clean_name": { "type": "string" },
                "confidence_score": { "type": "number" },
                "tax_relevant": { "type": "boolean" },
                "reasoning": { "type": "string" },
                "identified_member": { "type": ["string", "null"] }
            },
            "required": ["suggested_subfolder", "category", "correspondent", "new_clean_name", "confidence_score", "tax_relevant", "reasoning", "identified_member"]
        },
        "options": {
            "temperature": 0.0,
            "top_p": 0.1
        },
        "keep_alive": "5m"
    });

    let response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            #[derive(Deserialize)]
            struct OllamaMessage {
                content: String,
            }
            #[derive(Deserialize)]
            struct OllamaResponseWrapper {
                message: OllamaMessage,
            }

            let outer_json: OllamaResponseWrapper = resp
                .json()
                .await
                .map_err(|e| format!("Failed to read raw incoming network object: {}", e))?;

            let final_classification: AiClassificationResult = serde_json::from_str(&outer_json.message.content)
                .map_err(|e| {
                    format!(
                        "Received data broke schema layout contract: {}. Content: {}",
                        e, outer_json.message.content
                    )
                })?;

            Ok(final_classification)
        }
        err => {
            let error_msg = match err {
                Ok(resp) => format!("Ollama returned error: {}", resp.status()),
                Err(e) => format!("Ollama connection failure: {}", e),
            };
            eprintln!("Ollama failed, falling back to Gemini: {}", error_msg);
            request_gemini_classification(payload, gemini_api_key).await
        }
    }
}

async fn request_gemini_classification(
    payload: FileMetadataPayload,
    api_key: &str,
) -> Result<AiClassificationResult, String> {
    if api_key.is_empty() {
        return Err("Gemini API key is missing for fallback.".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash-lite:generateContent?key={}",
        api_key
    );

    let prompt = format!(
        "You are Tfatleek's backend filing clerk. Analyze the provided file metadata and text snippets. \
        Return a JSON object with: \
        - suggested_subfolder (string, snake_case) \
        - category (string) \
        - correspondent (string) \
        - new_clean_name (string) \
        - confidence_score (number, 0.0 to 1.0) \
        - tax_relevant (boolean) \
        - reasoning (string) \
        - identified_member (string or null, full name if matched)

        Family members: Tareg Mohamed Ahmed Shek (Father), Miluda Bashir Shek (Mother), Fatima Shek (Daughter), Sama Shek (Daughter).

        File Name: {}
        Size: {}
        Snippet Content:
        \"\"\"
        {}
        \"\"\"

        Return ONLY the raw JSON object.",
        payload.file_name, payload.file_size_formatted, payload.textual_snippet
    );

    let body = serde_json::json!({
        "contents": [{
            "parts": [{
                "text": prompt
            }]
        }],
        "generationConfig": {
            "response_mime_type": "application/json"
        }
    });

    let response = client.post(url).json(&body).send().await
        .map_err(|e| format!("Gemini API request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Gemini API returned error: {}", response.status()));
    }

    let gemini_resp: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

    let content = gemini_resp["candidates"][0]["content"]["parts"][0]["text"].as_str()
        .ok_or("Invalid Gemini response format")?;

    let result: AiClassificationResult = serde_json::from_str(content)
        .map_err(|e| format!("Failed to parse Gemini JSON: {}. Content: {}", e, content))?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    // Tests for local AI integration would typically require a running Ollama instance.
    // We can add mock tests if necessary in the future.
}
