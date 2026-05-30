use std::path::Path;
use ssh2::{Session, Sftp};
use std::net::TcpStream;

pub struct AsustorNasBridge {
    nas_host: String,
    username: String,
}

impl AsustorNasBridge {
    pub fn new(host: &str, user: &str) -> Self {
        Self {
            nas_host: host.to_string(),
            username: user.to_string(),
        }
    }

    /// Verifies authentication credentials and establishes a secure SFTP session matrix
    pub fn establish_sftp_session(&self, password: &str) -> Result<(Session, Sftp), String> {
        // Connect via standard port 22 network sockets
        let tcp = TcpStream::connect(format!("{}:22", self.nas_host))
            .map_err(|e| format!("Failed to resolve network route to NAS host target: {}", e))?;
        
        let mut sess = Session::new()
            .map_err(|e| format!("Unable to instantiate secure SSH2 runtime layer: {}", e))?;
        
        sess.set_tcp_stream(tcp);
        sess.handshake()
            .map_err(|e| format!("SSH handshake protocol structural collision: {}", e))?;

        sess.userauth_password(&self.username, password)
            .map_err(|e| format!("Asustor Access Refused - Authentication Credential Failure: {}", e))?;

        if !sess.authenticated() {
            return Err("Security Validation Check Rejected: Session unauthenticated.".to_string());
        }

        let sftp = sess.sftp()
            .map_err(|e| format!("SFTP Subsystem initialization rejected by remote server: {}", e))?;

        Ok((sess, sftp))
    }

    /// Crawls a target folder on the Asustor NAS filesystem using the active SFTP stream pipeline
    pub fn list_remote_directory(&self, sftp: &Sftp, remote_path: &str) -> Result<Vec<(String, u64)>, String> {
        let path = Path::new(remote_path);
        let readdir_results = sftp.readdir(path)
            .map_err(|e| format!("Remote system listing directory failure query error: {}", e))?;

        let mut discovered_files = Vec::new();

        for (file_path, file_stat) in readdir_results {
            if file_stat.is_file() {
                let absolute_path_string = file_path.to_string_lossy().to_string();
                let size_bytes = file_stat.size.unwrap_or(0);
                discovered_files.push((absolute_path_string, size_bytes));
            }
        }

        Ok(discovered_files)
    }
}
