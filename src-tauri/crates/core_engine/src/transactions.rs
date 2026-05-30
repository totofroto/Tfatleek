use std::fs;
use std::path::Path;
use database::DbManager;

pub struct SafeFileSystemEngine {
    db_path: String,
}

impl SafeFileSystemEngine {
    pub fn new(db_path: &str) -> Self {
        Self { db_path: db_path.to_string() }
    }

    /// Safely executes a file displacement (Move/Rename) backed by the pre-flight safety ledger
    pub fn execute_safe_move(&self, src: &str, dest: &str) -> Result<(), String> {
        let src_path = Path::new(src);
        let dest_path = Path::new(dest);

        if !src_path.exists() {
            return Err(format!("Source path does not exist: {}", src));
        }

        // Create target directory tree structures if missing
        if let Some(parent) = dest_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| format!("Folder structure creation failed: {}", e))?;
            }
        }

        let db = DbManager::init(&self.db_path).map_err(|e| e.to_string())?;
        let mut tx_id: i64 = 0;

        // Phase 1: Register PENDING log entry to SQLite
        if let Ok(conn_lock) = db.conn.lock() {
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            conn_lock.execute(
                "INSERT INTO transaction_log (operation_type, source_path, destination_path, timestamp, status) VALUES ('MOVE', ?1, ?2, ?3, 'PENDING')",
                rusqlite::params![src, dest, current_time],
            ).map_err(|e| format!("Pre-flight safety ledger write aborted: {}", e))?;
            
            tx_id = conn_lock.last_insert_rowid();
        }

        // Phase 2: Native OS File Manipulation
        match fs::rename(src_path, dest_path) {
            Ok(_) => {
                // Commit complete status
                if let Ok(conn_lock) = db.conn.lock() {
                    let _ = conn_lock.execute(
                        "UPDATE transaction_log SET status = 'COMPLETED' WHERE id = ?1",
                        rusqlite::params![tx_id],
                    );
                    // Dynamically update path map inside file_index table as well
                    let _ = conn_lock.execute(
                        "UPDATE file_index SET file_path = ?, file_name = ? WHERE file_path = ?",
                        rusqlite::params![
                            dest, 
                            dest_path.file_name().and_then(|n| n.to_str()).unwrap_or(""), 
                            src
                        ],
                    );
                }
                Ok(())
            }
            Err(os_err) => {
                // Flag failure to prevent structural sync bugs
                if let Ok(conn_lock) = db.conn.lock() {
                    let _ = conn_lock.execute(
                        "UPDATE transaction_log SET status = 'FAILED' WHERE id = ?1",
                        rusqlite::params![tx_id],
                    );
                }
                Err(format!("OS file system migration failure: {}", os_err))
            }
        }
    }

    /// Read transaction logs in reverse to safely return files to their original coordinates
    pub fn execute_undo_last_transaction(&self) -> Result<String, String> {
        let db = DbManager::init(&self.db_path).map_err(|e| e.to_string())?;
        
        let mut last_tx: Option<(i64, String, String, String)> = None;

        if let Ok(conn_lock) = db.conn.lock() {
            last_tx = conn_lock.query_row(
                "SELECT id, operation_type, source_path, destination_path FROM transaction_log WHERE status = 'COMPLETED' ORDER BY id DESC LIMIT 1",
                rusqlite::params![],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            ).ok();
        }

        if let Some((id, op_type, original_src, original_dest)) = last_tx {
            if op_type == "MOVE" {
                // Reverse coordinates: Move from current location (original_dest) back to origin (original_src)
                let current_loc = Path::new(&original_dest);
                let origin_loc = Path::new(&original_src);

                if !current_loc.exists() {
                    return Err(format!("Undo target file has been externally modified or removed: {}", original_dest));
                }

                fs::rename(current_loc, origin_loc).map_err(|e| format!("Undo migration sequence failed: {}", e))?;

                // Mark entry as ROLLEDBACK in our security tracking table
                if let Ok(conn_lock) = db.conn.lock() {
                    let _ = conn_lock.execute(
                        "UPDATE transaction_log SET status = 'ROLLEDBACK' WHERE id = ?1",
                        rusqlite::params![id],
                    );
                    let _ = conn_lock.execute(
                        "UPDATE file_index SET file_path = ?, file_name = ? WHERE file_path = ?",
                        rusqlite::params![
                            original_src, 
                            origin_loc.file_name().and_then(|n| n.to_str()).unwrap_or(""), 
                            original_dest
                        ],
                    );
                }
                return Ok(format!("Successfully rolled back operation ID {}: File returned to {}", id, original_src));
            }
        }
        Err("No valid, reversible transaction profiles discovered inside system logs.".to_string())
    }
}
