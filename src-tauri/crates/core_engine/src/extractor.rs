use std::fs::File;
use std::io::{Read, Seek, SeekFrom, BufReader};
use std::path::Path;

/// Enhanced bounded extraction mapping text files, medical DICOM formats, 
/// and complex rich media types into clean text strings for AI context frames.
pub fn extract_file_snippet<P: AsRef<Path>>(file_path: P) -> String {
    let path = file_path.as_ref();
    let extension = path.extension()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    match extension.to_lowercase().as_str() {
        "dcm" | "dicom" => parse_dicom_header(path),
        "pdf"           => parse_pdf_document(path),
        "docx"          => parse_docx_document(path),
        _               => parse_plain_text(path),
    }
}

/// Native PDF content stream extractor wrapper bounded strictly at 500KB or first 3 pages
fn parse_pdf_document(path: &Path) -> String {
    match pdf_oxide::PdfDocument::open(path) {
        Ok(doc) => {
            let mut collected_text = String::with_capacity(2048);
            let page_count = doc.page_count().unwrap_or(0);
            let max_pages = page_count.min(3);
            
            for i in 0..max_pages {
                if let Ok(text) = doc.extract_text(i) {
                    collected_text.push_str(&text);
                    collected_text.push(' ');
                }
                // Also enforce the 500kb limit (approx 500k characters for simplicity in this context)
                if collected_text.len() >= 500_000 { break; }
            }
            
            if collected_text.trim().is_empty() {
                "Error: Document layout contains zero indexable or rendered text strings.".to_string()
            } else {
                format!("[EXTRACTED PDF TEXT DOC CONTEXT]: {}", collected_text.trim())
            }
        }
        Err(_) => "[SYSTEM ERROR]: Corrupt or encrypted PDF data stream bypassed.".to_string(),
    }
}

/// MS Word Docx open-XML element text engine wrapper bounded at 500KB
fn parse_docx_document(path: &Path) -> String {
    match docx_lite::extract_text(path) {
        Ok(mut body_text) => {
            if body_text.len() > 500_000 {
                body_text.truncate(500_000);
            }
            format!("[EXTRACTED WORD DOC CONTEXT]: {}", body_text.trim())
        }
        Err(_) => "[SYSTEM ERROR]: Failed to decode target MS Office open-XML file structure.".to_string(),
    }
}

/// Reads plain text files, logs, or scripts up to a strict 500KB limit
fn parse_plain_text(path: &Path) -> String {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return "Error: Unable to open target file matrix.".to_string(),
    };
    
    let mut reader = BufReader::new(file);
    let mut buffer = vec![0; 500_000];
    
    match reader.read(&mut buffer) {
        Ok(bytes_read) => {
            String::from_utf8_lossy(&buffer[..bytes_read])
                .trim()
                .to_string()
        }
        Err(_) => "Error: Stream read failure during parsing operation.".to_string(),
    }
}

/// Fast medical DICOM file header parser
fn parse_dicom_header(path: &Path) -> String {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return "Error: Unable to open DICOM dataset.".to_string(),
    };

    let mut preamble = [0; 132];
    if file.read_exact(&mut preamble).is_err() {
        return "Error: Truncated medical file header format.".to_string();
    }

    if &preamble[128..132] != b"DICM" {
        let _ = file.seek(SeekFrom::Start(0));
        return parse_plain_text(path);
    }

    let mut header_buf = vec![0; 4096];
    let bytes_read = file.read(&mut header_buf).unwrap_or(0);
    
    let mut metadata_strings = Vec::new();
    let mut current_string = Vec::new();

    for &byte in &header_buf[..bytes_read] {
        if byte.is_ascii_graphic() || byte == b' ' {
            current_string.push(byte);
        } else {
            if current_string.len() > 3 {
                if let Ok(valid_str) = String::from_utf8(current_string.clone()) {
                    let cleaned = valid_str.trim().to_string();
                    if !cleaned.is_empty() && cleaned.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_') {
                        metadata_strings.push(cleaned);
                    }
                }
            }
            current_string.clear();
        }
        if metadata_strings.len() >= 25 { break; }
    }

    format!(
        "[MEDICAL DICOM PATIENT IMAGING DATA] Signature Verified. Structural Tags Found: {}", 
        metadata_strings.join(", ")
    )
}
