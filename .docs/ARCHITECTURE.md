# Technical Architecture & Constraints

## 1. IPC Boundary & Threading Laws
- **Rule:** Never block the Tauri Main Thread. 
- All intensive file IO, BLAKE3 hashing, and AI clustering operations must be dispatched to Rust background worker threads via `tokio::spawn` or scoped `rayon` thread pools.
- The IPC layer (`src-tauri/src/lib.rs`) handles serialization (`serde`) and passes tasks down immediately to background worker sub-crates.

## 2. Memory & RAM Constraints
- **Stream Everything:** Never read entire large files into memory for processing or hashing.
- File comparisons must chunk IO stream arrays strictly bounded at 1MB blocks max to preserve a small memory profile, regardless of the target dataset size.

## 3. Storage Performance
- Data persistence utilizes an embedded SQLite engine. 
- Fast indexing must be maintained via composite schema keys on file hash, paths, and modification timestamps.
