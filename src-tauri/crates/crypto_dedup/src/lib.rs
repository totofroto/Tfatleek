use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use blake3::Hasher;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct ScanResult {
    pub file_path: String,
    pub file_name: String,
    pub file_size: u64,
    pub modified_at: u64,
    pub partial_hash: Option<String>,
    pub full_hash: Option<String>,
}

pub fn is_safe_to_scan(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();
    
    // Non-negotiable protective exclusion array
    let blocked_patterns = [
        "node_modules", ".git", "target", "src-tauri", 
        "/library/", "/system/", "/applications/", ".trash"
    ];
    
    !blocked_patterns.iter().any(|pattern| path_str.contains(pattern))
}

pub fn run_dedup_scan<P: AsRef<Path>>(root_path: P) -> Vec<ScanResult> {
    // Stage 1: Size Aggregation
    let mut size_groups: HashMap<u64, Vec<PathBuf>> = HashMap::new();

    for entry in WalkDir::new(root_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_safe_to_scan(e.path()))
    {
        let path = entry.path().to_path_buf();
        if let Ok(metadata) = entry.metadata() {
            let size = metadata.len();
            size_groups.entry(size).or_default().push(path);
        }
    }

    // Filter out groups with only one file
    let candidate_paths: Vec<(u64, Vec<PathBuf>)> = size_groups
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .collect();

    // Stage 2: 4KB Partial Hashing
    let partial_hash_results: Vec<ScanResult> = candidate_paths
        .into_par_iter()
        .flat_map(|(size, paths)| {
            let mut partial_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
            
            for path in paths {
                if let Ok(p_hash) = compute_partial_hash(&path) {
                    partial_groups.entry(p_hash).or_default().push(path);
                }
            }

            partial_groups
                .into_iter()
                .flat_map(|(p_hash, paths)| {
                    paths.into_iter().map(move |path| {
                        let metadata = path.metadata().ok();
                        ScanResult {
                            file_name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                            file_path: path.to_string_lossy().into_owned(),
                            file_size: size,
                            modified_at: metadata.and_then(|m| m.modified().ok())
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_secs())
                                .unwrap_or(0),
                            partial_hash: Some(p_hash.clone()),
                            full_hash: None, // Will be computed in Stage 3 if needed
                        }
                    }).collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect();

    // Stage 3: Full BLAKE3 Stream Hashing
    // We only need full hashes for files that share the same (size, partial_hash) with others
    let mut final_groups: HashMap<(u64, String), Vec<ScanResult>> = HashMap::new();
    for res in partial_hash_results {
        if let Some(ref p_hash) = res.partial_hash {
            final_groups.entry((res.file_size, p_hash.clone())).or_default().push(res);
        }
    }

    final_groups
        .into_par_iter()
        .flat_map(|(_, mut results)| {
            if results.len() > 1 {
                for res in &mut results {
                    if let Ok(f_hash) = compute_full_hash(Path::new(&res.file_path)) {
                        res.full_hash = Some(f_hash);
                    }
                }
            }
            results
        })
        .collect()
}

fn compute_partial_hash(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut buffer = [0; 4096];
    let n = file.read(&mut buffer)?;
    let hash = blake3::hash(&buffer[..n]);
    Ok(hash.to_hex().to_string())
}

fn compute_full_hash(path: &Path) -> std::io::Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(1_048_576, file);
    let mut hasher = Hasher::new();
    let mut buffer = [0; 1_048_576];

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_dedup_scan() {
        let test_dir = std::env::temp_dir().join("tfatleek_test_dedup");
        if test_dir.exists() {
            fs::remove_dir_all(&test_dir).unwrap();
        }
        fs::create_dir_all(&test_dir).unwrap();

        let file1 = test_dir.join("file1.txt");
        let file2 = test_dir.join("file2.txt");
        let file3 = test_dir.join("file3.txt");

        let content = b"duplicate content";
        fs::write(&file1, content).unwrap();
        fs::write(&file2, content).unwrap();
        fs::write(&file3, b"unique content").unwrap();

        let results = run_dedup_scan(&test_dir);
        
        // We expect file1 and file2 to have full hashes and be in the results
        // file3 might be in results if its size matched others, but here sizes differ.
        // Actually file1 and file2 have same size, file3 has different size.
        // Stage 1 will only keep file1 and file2.
        
        let duplicates: Vec<_> = results.iter().filter(|r| r.full_hash.is_some()).collect();
        assert_eq!(duplicates.len(), 2);
        assert_eq!(duplicates[0].full_hash, duplicates[1].full_hash);
        
        fs::remove_dir_all(&test_dir).unwrap();
    }
}
