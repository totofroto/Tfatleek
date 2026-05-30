use std::net::TcpStream;
use ssh2::Session;
use std::path::Path;

pub struct NasConnectionProfile {
    pub host: String,
    pub user: String,
}

impl NasConnectionProfile {
    pub fn scan_remote_nas_directory(&self, remote_dir: &str) -> Result<Vec<String>, String> {
        // Absolute Network Guardrail: Never let an unauthenticated or raw root call execute
        if remote_dir == "/" || remote_dir.is_empty() {
            return Err("Safety Block: Cannot target remote network root mounts directly.".to_string());
        }

        let tcp = TcpStream::connect(format!("{}:22", self.host))
            .map_err(|e| format!("Network Connection Failure: {}", e))?;
        
        let mut sess = Session::new().unwrap();
        sess.set_tcp_stream(tcp);
        sess.handshake().map_err(|e| format!("SSH Handshake Failed: {}", e))?;
        
        // This will hook seamlessly into your secure local SSH agent keys on macOS
        sess.userauth_agent(&self.user).map_err(|e| format!("Authentication Rejected: {}", e))?;

        let sftp = sess.sftp().map_err(|e| format!("SFTP Subsystem Initialization Failed: {}", e))?;
        let remote_path = Path::new(remote_dir);
        let readdir = sftp.readdir(remote_path).map_err(|e| format!("Failed to read remote array: {}", e))?;

        let mut structured_remote_files = Vec::new();
        let approved_white_list = ["pdf", "docx", "doc", "txt", "log", "dcm", "dicom"];

        for (path, stat) in readdir {
            if stat.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                
                // ST_INCLUSION GUARD: Filter out .DS_Store, index elements, and media logs at the remote threshold
                if approved_white_list.contains(&ext.as_str()) {
                    if let Some(p_str) = path.to_str() {
                        structured_remote_files.push(format!("sftp://{}/{}", self.host, p_str));
                    }
                }
            }
        }

        Ok(structured_remote_files)
    }
}
