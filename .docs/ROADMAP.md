# Tfatleek Project Roadmap & Milestones

This document dictates the non-negotiable sequence of development for Tfatleek. Do not proceed to a later phase until the current phase is fully tested and structurally signed off.

## Phase 1: Foundations & High-Speed Dedup Engine (COMPLETED)
- [x] Tauri v2 project initialization with React, TS, Tailwind, and a Modular Rust workspace.
- [x] SQLite Database state wrapper using `rusqlite` for transaction logging and configuration storage.
- [x] **Crypto Dedup Sub-crate:** High-performance, multi-threaded directory walking (`rayon`) executing size grouping -> 4KB partial hash -> full BLAKE3 cryptographic hash.
- [x] **React Dashboard:** Integrated control interface for triggering scans and visualizing file manifests via Tauri IPC.

## Phase 2: Local AI Metadata & Embedding Engine
- [ ] Text extraction service for system files (.txt, .pdf, .docx) using low-overhead native Rust tools.
- [ ] Local vector embedding pipeline using Hugging Face `candle` or ONNX Runtime to execute `all-MiniLM-L6-v2` 100% on-device.
- [ ] K-Means clustering engine to group untagged files mathematically into suggested target directories.

## Phase 3: Rule Orchestration & Safe Transactions
- [ ] Visual Rule UI engine configuration parser (JSON schemas mapping visual node trees to execution criteria).
- [ ] Transaction Engine: Every file operation (Move, Rename, Shred) must write a pre-flight log entry to SQLite to support atomic Rollback ("Undo").
- [ ] Secure shredding engine featuring standard sanitization routines.

## Phase 4: Remote Infrastructure (Asustor NAS Bridge)
- [ ] Core integration of native Rust SSH2/SFTP clients.
- [ ] Direct execution over network paths allowing identical background analysis behavior across local Mac Storage and remote SMB volumes.
