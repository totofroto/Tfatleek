use std::sync::{Arc, Mutex};
use rusqlite::{params, Connection, Result};

pub struct DbManager {
    pub conn: Arc<Mutex<Connection>>,
}

pub struct FileRecord {
    pub id: Option<i64>,
    pub file_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub modified_at: i64,
    pub partial_hash: Option<String>,
    pub full_hash: Option<String>,
    pub is_nas_path: bool,
}

impl DbManager {
    pub fn init(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        
        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON;", [])?;

        // Create tables
        conn.execute_batch(
            "BEGIN;
            CREATE TABLE IF NOT EXISTS file_index (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT UNIQUE NOT NULL,
                file_name TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                modified_at INTEGER NOT NULL,
                partial_hash TEXT,
                full_hash TEXT,
                is_nas_path INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_file_index_file_path ON file_index(file_path);
            CREATE INDEX IF NOT EXISTS idx_file_index_file_size ON file_index(file_size);
            CREATE INDEX IF NOT EXISTS idx_file_index_full_hash ON file_index(full_hash);

            CREATE TABLE IF NOT EXISTS ai_metadata (
                file_id INTEGER PRIMARY KEY,
                extracted_text TEXT,
                suggested_subfolder TEXT,
                confidence_score REAL DEFAULT 0.0,
                is_tax_relevant INTEGER DEFAULT 0,
                identified_member TEXT,
                ai_processed_at INTEGER,
                FOREIGN KEY(file_id) REFERENCES file_index(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS transaction_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                operation_type TEXT NOT NULL,
                source_path TEXT NOT NULL,
                destination_path TEXT,
                timestamp INTEGER NOT NULL,
                status TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS file_knowledge_graph (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                blake3_hash TEXT UNIQUE,
                file_name TEXT,
                last_known_path TEXT,
                inferred_category TEXT,
                confidence_score REAL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            COMMIT;"
        )?;

        Ok(DbManager {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn upsert_file(&self, record: &FileRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO file_index (
                file_path, file_name, file_size, modified_at, partial_hash, full_hash, is_nas_path
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(file_path) DO UPDATE SET
                file_name=excluded.file_name,
                file_size=excluded.file_size,
                modified_at=excluded.modified_at,
                partial_hash=excluded.partial_hash,
                full_hash=excluded.full_hash,
                is_nas_path=excluded.is_nas_path",
            params![
                record.file_path,
                record.file_name,
                record.file_size,
                record.modified_at,
                record.partial_hash,
                record.full_hash,
                if record.is_nas_path { 1 } else { 0 }
            ],
        )?;
        Ok(())
    }

    pub fn find_duplicates(&self) -> Result<Vec<(String, i64, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT full_hash, file_size, group_concat(file_path, '|') 
             FROM file_index 
             WHERE full_hash IS NOT NULL 
             GROUP BY full_hash, file_size 
             HAVING count(*) > 1"
        )?;
        
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_init_and_upsert() -> Result<()> {
        let db = DbManager::init(":memory:")?;
        
        let record = FileRecord {
            id: None,
            file_path: "/path/to/file.txt".to_string(),
            file_name: "file.txt".to_string(),
            file_size: 1024,
            modified_at: 123456789,
            partial_hash: Some("abc".to_string()),
            full_hash: Some("abcdef".to_string()),
            is_nas_path: false,
        };

        db.upsert_file(&record)?;
        
        // Test upsert (update)
        let record2 = FileRecord {
            id: None,
            file_path: "/path/to/file.txt".to_string(),
            file_name: "file_new.txt".to_string(),
            file_size: 2048,
            modified_at: 123456790,
            partial_hash: Some("abc2".to_string()),
            full_hash: Some("abcdef2".to_string()),
            is_nas_path: true,
        };
        db.upsert_file(&record2)?;

        Ok(())
    }

    #[test]
    fn test_find_duplicates() -> Result<()> {
        let db = DbManager::init(":memory:")?;
        
        db.upsert_file(&FileRecord {
            id: None,
            file_path: "/path1".to_string(),
            file_name: "f1".to_string(),
            file_size: 100,
            modified_at: 1,
            partial_hash: None,
            full_hash: Some("hash1".to_string()),
            is_nas_path: false,
        })?;

        db.upsert_file(&FileRecord {
            id: None,
            file_path: "/path2".to_string(),
            file_name: "f2".to_string(),
            file_size: 100,
            modified_at: 1,
            partial_hash: None,
            full_hash: Some("hash1".to_string()),
            is_nas_path: false,
        })?;

        let dups = db.find_duplicates()?;
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].0, "hash1");
        assert!(dups[0].2.contains("/path1"));
        assert!(dups[0].2.contains("/path2"));

        Ok(())
    }
}
