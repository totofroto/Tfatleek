use std::sync::{Arc, Mutex};
use rusqlite::{params, Connection, Result};
use serde::{Serialize, Deserialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartGroup {
    pub id: i64,
    pub name: String,
    pub icon: String,
    pub filter_category: Option<String>,
    pub filter_tax_relevant: Option<i64>,
    pub filter_year: Option<i64>,
    pub filter_correspondent: Option<String>,
    pub filter_min_confidence: f64,
    pub created_at: String,
    pub sort_order: i64,
    pub file_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSmartGroup {
    pub name: String,
    pub icon: String,
    pub filter_category: Option<String>,
    pub filter_tax_relevant: Option<bool>,
    pub filter_year: Option<i64>,
    pub filter_correspondent: Option<String>,
    pub filter_min_confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIndexEntry {
    pub id: i64,
    pub file_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub modified_at: i64,
    pub category: Option<String>,
    pub correspondent: Option<String>,
    pub confidence_score: f64,
    pub tax_relevant: bool,
    pub suggested_subfolder: Option<String>,
    pub identified_member: Option<String>,
}

impl DbManager {
    pub fn init(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;

        conn.execute("PRAGMA foreign_keys = ON;", [])?;
        conn.pragma_update(None, "journal_mode", &"WAL")?;
        conn.pragma_update(None, "synchronous", &"NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

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
                category TEXT,
                correspondent TEXT,
                confidence_score REAL DEFAULT 0.0,
                tax_relevant INTEGER DEFAULT 0,
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

            CREATE TABLE IF NOT EXISTS file_transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                batch_id TEXT NOT NULL,
                operation_type TEXT NOT NULL,
                source_path TEXT NOT NULL,
                destination_path TEXT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
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

            CREATE TABLE IF NOT EXISTS smart_groups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                icon TEXT NOT NULL DEFAULT '📁',
                filter_category TEXT,
                filter_tax_relevant INTEGER,
                filter_year INTEGER,
                filter_correspondent TEXT,
                filter_min_confidence REAL DEFAULT 0.0,
                created_at TEXT NOT NULL,
                sort_order INTEGER DEFAULT 0
            );

            INSERT OR IGNORE INTO smart_groups
                (name, icon, filter_category, filter_tax_relevant, created_at, sort_order)
            VALUES
                ('All Documents', '📄', NULL, NULL, datetime('now'), 0),
                ('Tax Documents', '💰', 'Financial', 1, datetime('now'), 1),
                ('Medical Records', '🏥', 'Medical', NULL, datetime('now'), 2),
                ('Unsorted', '📥', '_Unsorted', NULL, datetime('now'), 3);

            COMMIT;"
        )?;

        // Non-destructive migration: ai_metadata columns
        {
            let mut stmt = conn.prepare("PRAGMA table_info(ai_metadata);")?;
            let columns: Vec<String> = stmt.query_map([], |row| row.get::<_, String>(1))?
                .filter_map(|r| r.ok())
                .collect();

            if !columns.contains(&"category".to_string()) {
                conn.execute("ALTER TABLE ai_metadata ADD COLUMN category TEXT;", [])?;
            }
            if !columns.contains(&"correspondent".to_string()) {
                conn.execute("ALTER TABLE ai_metadata ADD COLUMN correspondent TEXT;", [])?;
            }
            if !columns.contains(&"tax_relevant".to_string()) {
                conn.execute("ALTER TABLE ai_metadata ADD COLUMN tax_relevant INTEGER DEFAULT 0;", [])?;
                if columns.contains(&"is_tax_relevant".to_string()) {
                    conn.execute("UPDATE ai_metadata SET tax_relevant = is_tax_relevant WHERE tax_relevant = 0;", [])?;
                }
            }
        }

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

    pub fn get_smart_groups(&self) -> Result<Vec<SmartGroup>> {
        let conn = self.conn.lock().unwrap();

        // Collect raw group data in an inner block so stmt is dropped before reusing conn
        let groups_raw: Vec<(i64, String, String, Option<String>, Option<i64>, Option<i64>, Option<String>, f64, String, i64)> = {
            let mut stmt = conn.prepare(
                "SELECT id, name, icon, filter_category, filter_tax_relevant, filter_year,
                        filter_correspondent, filter_min_confidence, created_at, sort_order
                 FROM smart_groups ORDER BY sort_order ASC, id ASC"
            )?;
            stmt.query_map([], |row| Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
            )))?.collect::<Result<Vec<_>>>()?
        };

        let mut result = Vec::new();
        for (id, name, icon, filter_category, filter_tax_relevant, filter_year, filter_correspondent, filter_min_confidence, created_at, sort_order) in groups_raw {
            let year_str = filter_year.map(|y| y.to_string());
            let file_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM file_index fi
                 LEFT JOIN ai_metadata am ON fi.id = am.file_id
                 WHERE (?1 IS NULL OR am.category = ?1)
                   AND (?2 IS NULL OR am.tax_relevant = ?2)
                   AND (COALESCE(am.confidence_score, 0.0) >= ?3)
                   AND (?4 IS NULL OR strftime('%Y', datetime(fi.modified_at, 'unixepoch')) = ?4)
                   AND (?5 IS NULL OR am.correspondent = ?5)",
                params![filter_category, filter_tax_relevant, filter_min_confidence, year_str, filter_correspondent],
                |row| row.get(0)
            ).unwrap_or(0);

            result.push(SmartGroup {
                id, name, icon, filter_category, filter_tax_relevant, filter_year,
                filter_correspondent, filter_min_confidence, created_at, sort_order, file_count,
            });
        }

        Ok(result)
    }

    pub fn get_smart_group_files(&self, group_id: i64) -> Result<Vec<FileIndexEntry>> {
        let conn = self.conn.lock().unwrap();

        let (filter_category, filter_tax_relevant, filter_year, filter_correspondent, filter_min_confidence):
            (Option<String>, Option<i64>, Option<i64>, Option<String>, f64) = conn.query_row(
            "SELECT filter_category, filter_tax_relevant, filter_year,
                    filter_correspondent, filter_min_confidence
             FROM smart_groups WHERE id = ?1",
            params![group_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        )?;

        let year_str = filter_year.map(|y| y.to_string());

        let mut stmt = conn.prepare(
            "SELECT fi.id, fi.file_path, fi.file_name, fi.file_size, fi.modified_at,
                    COALESCE(am.category, ''), COALESCE(am.correspondent, ''),
                    COALESCE(am.confidence_score, 0.0), COALESCE(am.tax_relevant, 0),
                    am.suggested_subfolder, am.identified_member
             FROM file_index fi
             LEFT JOIN ai_metadata am ON fi.id = am.file_id
             WHERE (?1 IS NULL OR am.category = ?1)
               AND (?2 IS NULL OR am.tax_relevant = ?2)
               AND (COALESCE(am.confidence_score, 0.0) >= ?3)
               AND (?4 IS NULL OR strftime('%Y', datetime(fi.modified_at, 'unixepoch')) = ?4)
               AND (?5 IS NULL OR am.correspondent = ?5)
             ORDER BY fi.file_name ASC"
        )?;

        let entries = stmt.query_map(
            params![filter_category, filter_tax_relevant, filter_min_confidence, year_str, filter_correspondent],
            |row| {
                let cat: String = row.get(5)?;
                let corr: String = row.get(6)?;
                Ok(FileIndexEntry {
                    id: row.get(0)?,
                    file_path: row.get(1)?,
                    file_name: row.get(2)?,
                    file_size: row.get(3)?,
                    modified_at: row.get(4)?,
                    category: if cat.is_empty() { None } else { Some(cat) },
                    correspondent: if corr.is_empty() { None } else { Some(corr) },
                    confidence_score: row.get(7)?,
                    tax_relevant: row.get::<_, i32>(8)? != 0,
                    suggested_subfolder: row.get(9)?,
                    identified_member: row.get(10)?,
                })
            }
        )?.collect::<Result<Vec<_>>>()?;

        Ok(entries)
    }

    pub fn create_smart_group(&self, group: NewSmartGroup) -> Result<SmartGroup> {
        let conn = self.conn.lock().unwrap();

        let tax_flag = group.filter_tax_relevant.map(|b| if b { 1i64 } else { 0i64 });

        conn.execute(
            "INSERT INTO smart_groups
                (name, icon, filter_category, filter_tax_relevant, filter_year,
                 filter_correspondent, filter_min_confidence, created_at, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'),
                     (SELECT COALESCE(MAX(sort_order), 3) + 1 FROM smart_groups))",
            params![
                group.name,
                group.icon,
                group.filter_category,
                tax_flag,
                group.filter_year,
                group.filter_correspondent,
                group.filter_min_confidence,
            ]
        )?;

        let id = conn.last_insert_rowid();

        conn.query_row(
            "SELECT id, name, icon, filter_category, filter_tax_relevant, filter_year,
                    filter_correspondent, filter_min_confidence, created_at, sort_order
             FROM smart_groups WHERE id = ?1",
            params![id],
            |row| Ok(SmartGroup {
                id: row.get(0)?,
                name: row.get(1)?,
                icon: row.get(2)?,
                filter_category: row.get(3)?,
                filter_tax_relevant: row.get(4)?,
                filter_year: row.get(5)?,
                filter_correspondent: row.get(6)?,
                filter_min_confidence: row.get(7)?,
                created_at: row.get(8)?,
                sort_order: row.get(9)?,
                file_count: 0,
            })
        )
    }

    pub fn delete_smart_group(&self, group_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM smart_groups WHERE id = ?1", params![group_id])?;
        Ok(())
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

    #[test]
    fn test_smart_groups_defaults() -> Result<()> {
        let db = DbManager::init(":memory:")?;
        let groups = db.get_smart_groups()?;
        assert_eq!(groups.len(), 4);
        assert_eq!(groups[0].name, "All Documents");
        assert_eq!(groups[1].name, "Tax Documents");
        Ok(())
    }

    #[test]
    fn test_smart_group_create_delete() -> Result<()> {
        let db = DbManager::init(":memory:")?;
        let created = db.create_smart_group(NewSmartGroup {
            name: "My Group".to_string(),
            icon: "🗂️".to_string(),
            filter_category: Some("Legal".to_string()),
            filter_tax_relevant: None,
            filter_year: None,
            filter_correspondent: None,
            filter_min_confidence: 0.0,
        })?;
        assert_eq!(created.name, "My Group");
        assert!(created.id > 4);
        db.delete_smart_group(created.id)?;
        Ok(())
    }
}
