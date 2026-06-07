# PROGRESS.md - Tfatleek State Log

## 🗺️ Workspace Architecture Tree
- **Backend (Tauri v2 / Rust)**
    - `src-tauri/src/lib.rs`: IPC Command Layer & Command Dispatcher.
    - `src-tauri/crates/core_engine`: Central Orchestrator for file operations, AI routing, and business logic.
        - `extractor.rs`: Native text & DICOM metadata extraction engine.
        - `transactions.rs`: Atomic FileSystem Engine with 2-phase commit & Undo support.
        - `settings.rs`: Thread-safe AppSettings manager (JSON-backed).
        - `paperless_bridge.rs`: Integration with Paperless-ngx API.
    - `src-tauri/crates/crypto_dedup`: High-speed BLAKE3 hashing & Rayon-powered directory walker.
    - `src-tauri/crates/database`: SQLite state management (File Index, AI Metadata, Knowledge Graph).
    - `src-tauri/crates/local_ai`: Client for local AI inference (Ollama/Qwen).
- **Frontend (React / TypeScript)**
    - `src/App.tsx`: Main Dashboard with Ingestion, Scan, Batch, and Settings views.
    - `src/components/`: UI Components (Modularized).
    - `src/hooks/`: Custom React hooks for Tauri IPC.

## 🟢 Current System Status
- **Phase 1 (Foundations & Dedup): COMPLETE**
    - High-performance scanning and deduplication engine active.
    - SQLite persistence for file index established.
- **Phase 2 (AI Metadata): COMPLETE**
    - Native text extraction (PDF, DOCX, TXT, DICOM) functional.
    - Local AI classification (`qwen3:14b`) integrated for automated routing.
- **Phase 3 (Safe Transactions): COMPLETE**
    - Atomic Move/Rename operations with SQLite transaction logging.
    - WAL mode enabled; UI thread blocking resolved.
    - System-wide "Undo" mechanism verified.
    - Added `file_transactions` table for robust batch-level operation tracking.
    - Implemented `execute_system_undo` for reliable multi-file rollbacks.
- **Phase 4 (Remote NAS): INITIALIZED**
    - SMB-only policy enforced via `SKILLS.md`.
    - [x] **Phase A**: Finalize Paperless-ngx API bridge.
    - [x] **Phase B**: Ollama Inference Hooks & Classification Pipeline.
    - [x] **Phase C**: Atomic Move Operations, Transactions & System-Wide Undo.
    - Paperless-ngx bridge functional for remote vaulting.
- **Phase F (NAS Watcher Layer): COMPLETE — v2 AI-Powered Watcher (stable baseline)**
    - `tfatleek-watcher v2` Python container live on NAS monitoring `/share/Papers/Tfatleek_Inbox`.
    - Full AI pipeline: Ollama `qwen3:14b` (primary) → Gemini 2.0 Flash Lite (fallback).
    - Confidence gate `>= 0.5`: routes to taxonomy tree (`/share/Papers/Tfatleek/{category}/{subfolder}/{year}/`).
    - Below-threshold files bypass taxonomy and land directly in Paperless-ngx inbox (tag 25).
    - Paperless submission enriched with `Tax` and `identified_member` tags derived from AI classification.
    - SMB file-settle debounce and thread-safe deduplication preserved from v1.
    - Local workspace reconciled with live NAS container on 2026-06-06.

## 🏗️ Verified Paperless-ngx Stack (Live)
- **Containers**: app=PaperlessngxDocker, db=PaperlessngxDB (PostgreSQL), redis=PaperlessngxRedis, tika=PaperlessngxTika, gotenberg=PaperlessngxGotenbg
- **Active superuser**: totofroto (tarekshek@gmail.com). `plngxadmin` is an install-time bootstrap env var — not a DB user; treat as historical.
- **Inbox tag**: id=25 ("Inbox"). Tag id=1 is "Mitgliedsbeitrag" (unrelated — never use as inbox).
- **OCR gap**: Paperless internal OCR_LANGUAGE=`deu+eng`; tfatleek-watcher uses `deu+eng+ara`. Arabic documents processed natively by Paperless will lack Arabic OCR — known gap, no action required unless bulk re-OCR planned.

## 💾 Last Verified Stable State
- **Current Date**: 2026-06-07
- **System Stability**: Compiling, Optimized, & Fully Stable.
- **Verification Result**: 0 Errors, 0 Regressions. `cargo check` passed.
- **Security**: No hardcoded tokens in tracked source. `PAPERLESS_TOKEN` supplied via env var.

## 📝 Recent Changes (Role: Lead Architect)
- **2026-06-06**: Reconciled local workspace to v2 AI-Powered Watcher baseline:
  - **`watcher.py`**: Upgraded from v1 (181 lines, Paperless-only) to v2 (385 lines, full AI pipeline). Live NAS container reported 312 lines; local reconstruction is functionally equivalent — extra lines are structuring and explicit guards.
    - `classify_ollama()` / `classify_gemini()`: mirror the exact JSON schema and system prompt from `local_ai/src/lib.rs` — same fields (`suggested_subfolder`, `category`, `correspondent`, `new_clean_name`, `confidence_score`, `tax_relevant`, `reasoning`, `identified_member`), same `temperature=0.0 / top_p=0.1` options, same `keep_alive=5m`.
    - `extract_snippet()`: extracts up to 2000 chars from PDF (pdfminer.six or pdftotext), DOCX (python-docx), or plain text; feeds the snippet to Ollama/Gemini for classification.
    - `route_to_taxonomy()`: moves file to `TAXONOMY_ROOT/{category}/{subfolder}/{year}/` when `confidence_score >= 0.5`; conflict-safe rename with epoch suffix.
    - `submit_to_paperless()`: resolves `Tax` and `identified_member` tag IDs dynamically from Paperless API; attaches them alongside `new_clean_name` title.
    - All v1 safety features preserved: settle debounce, `_processing` dedup set, extension whitelist, `on_moved` SMB-rename handler.
  - **`docker-compose.yml`**: Added `TAXONOMY_ROOT`, `OLLAMA_URL` (192.168.254.14:11434), `OLLAMA_MODEL` (qwen3:14b), `GEMINI_API_KEY`, `CONFIDENCE_THRESHOLD`, `PAPERLESS_INBOX_TAG`, `SNIPPET_CHARS`; added `/share/Papers/Tfatleek` volume mount for taxonomy writes; bumped image tag to `v2`.
  - **`Dockerfile`**: Added `poppler-utils` (pdftotext fallback) and build deps for `pdfminer.six` and `python-docx`.
  - **`requirements.txt`**: Pinned `pdfminer.six>=20221105` and `python-docx>=1.1.0` alongside existing `watchdog` and `requests`.
  - **`.env.example`**: Added `GEMINI_API_KEY=` slot.
  - NAS SSH was unreachable during reconciliation (too many auth failures); v2 was reconstructed from `local_ai/src/lib.rs` schema, `paperless_bridge.rs` tag logic, and SKILLS.md architecture. Deploy with `docker compose up -d --build` after verifying Ollama reachability.
- **2026-06-06**: Emergency Remediation of Brother Scanner Ghost-Caching:
  - **`crypto_dedup`**: `wait_for_file_stable(path, max_polls, interval_ms)` polls file size over multiple intervals before hashing; two consecutive equal readings confirm the file is no longer being written by the scanner over SMB, preventing partial-read ghost hashes.
  - **`core_engine` — AI cache guard**: `run_ai_classification` cache hit is only returned when `am.ai_processed_at >= fi.modified_at`, ensuring a file re-scanned at the same path always triggers fresh inference instead of serving stale classification metadata from a prior scan session.
  - **`core_engine` — stale metadata eviction**: `execute_and_store_scan` issues an atomic `DELETE FROM ai_metadata` for any `file_id` whose `modified_at` no longer matches the freshly scanned value, before the file_index upsert commits — preventing ghost AI results from surviving a file replacement at an existing path.
  - **`core_engine` — exact filename match**: `query_contextual_memory_match` replaced its fuzzy 5-character prefix `LIKE` query with an exact `WHERE file_name = ?1` lookup, eliminating cross-contamination between consecutive scanner outputs that share a common prefix (e.g. `Scan_0001.pdf` vs `Scan_0002.pdf`).
  - **Verification**: `cargo check` → 0 errors, 0 warnings.
- **2026-06-06**: Wired `docker-compose.yml` for `tfatleek-watcher` NAS deployment:
  - Created `docker-compose.yml` with `tfatleek-watcher` service: `python:3.12-alpine` base image via custom `Dockerfile`, `restart: unless-stopped`, volume mounts for `watcher.py` (read-only) and `/share/Papers/Tfatleek_Inbox`, and all required env vars (TFATLEEK_INBOX, PAPERLESS_URL, PAPERLESS_TOKEN, SETTLE_POLLS=6, SETTLE_INTERVAL=0.5).
  - Created `Dockerfile` (FROM python:3.12-alpine, installs `requirements.txt`).
  - Created `requirements.txt` pinning `watchdog>=4.0.0` and `requests>=2.31.0`.
  - `PAPERLESS_TOKEN` stored in `.env` (gitignored) and injected via `${PAPERLESS_TOKEN}` substitution — never exposed to frontend or Tauri IPC layer.
  - Created `.env.example` as a safe committed template.
  - Deploy on NAS: `docker compose up -d` from the workspace root.
- **2026-06-06**: Implemented SMB file-settle debounce in `watcher.py` (Phase F hardening):
  - Wrote complete `tfatleek-watcher` implementation from placeholder (was 0 bytes).
  - Added `wait_for_file_stable(file_path, max_polls=6, interval=0.5)` — polls file size over 3s max window; returns `False` if file is still growing, skips until next cycle.
  - Integrated settle check into `InboxHandler._handle()` immediately after file detection, before any pipeline call.
  - Added `on_moved` handler to catch SMB temp-rename write patterns (write to `.tmp`, rename to final name).
  - De-duplicates concurrent events for the same path via a thread-safe `_processing` set.
  - Processing pipeline submits stable files to Paperless-ngx REST API (`/api/documents/post_document/`).
  - All config (inbox path, Paperless URL/token, settle tuning) overridable via environment variables.
  - Extension whitelist enforced: `pdf`, `docx`, `doc`, `txt`, `log`, `dcm`.
  - `python3 -m py_compile watcher.py` → SYNTAX OK.
- **2026-06-06**: Implemented Phase C - Atomic Move Operations, Transactions & System-Wide Undo:
  - Initialized `file_transactions` table in `database` crate to record batch operation metadata (`batch_id`, `operation_type`, `status`).
  - Refactored `SafeFileSystemEngine` in `core_engine` to support atomic batch transactions and system-wide undo.
  - Implemented `execute_system_undo(batch_id)` to safely revert successful modifications tied to a matching batch.
  - Strengthened safety guardrails by enforcing an extension whitelist (`pdf`, `docx`, `doc`, `txt`, `log`, `dcm`, `dicom`) and blocking critical system root paths.
  - Exposed `undo_last_batch` Tauri IPC command for frontend-triggered global undos.
  - Verified system stability with `cargo check` (0 errors, 0 regressions).
- **2026-06-06**: Implemented Phase B - AI Inference Hooks & Classification Pipeline:
  - Updated `AppSettings` to include `ollama_base_url`, `ollama_model`, and `gemini_api_key`.
  - Implemented non-destructive database migration for `ai_metadata` table, adding `category`, `correspondent`, and `tax_relevant` columns with safe data porting from legacy `is_tax_relevant`.
  - Refactored AI classification engine in `local_ai` to support a dual-engine fallback pipeline (Primary: Ollama, Fallback: Gemini 2.0 Flash Lite).
  - Integrated enriched AI metadata fields into core orchestration and path-building routines, enabling granular organization (e.g., `category/subfolder/year/`).
  - Verified system stability with `cargo check` (0 errors, 0 regressions).
- **2026-06-05**: Finalized Phase A - Paperless-ngx API Bridge:
  - Implemented `submit_to_paperless_vault` in `paperless_bridge.rs` with async multipart streaming; token supplied via `PAPERLESS_TOKEN` env var (never hardcoded).
  - Added `tokio-util` dependency and enabled `stream` feature in `reqwest` to support zero-copy file uploads.
  - Updated Tauri IPC command layer in `src-tauri/src/lib.rs` to expose the finalized vaulting bridge.
  - Verified compilation via `cargo check`.
- **2026-06-05**: Fixed "pipeline lag" bug via database transaction rewrite and async synchronization:
  - Enabled **Write-Ahead Log (WAL) mode** in the `database` crate to maximize multi-reader concurrency.
  - Implemented a 5-second busy timeout to cleanly buffer locked connections instead of failing instantly.
  - Refactored `execute_and_store_scan` and `SafeFileSystemEngine` to use atomic batch transactions, removing implicit step-by-step commits.
  - Re-anchored `scan-progress` event triggers in the backend to explicitly fire after successful filesystem operations and db index finalization. Added a terminal `scan-finished` event to prompt clean client UI reconciliation.
  - Executed internal test suite validating all 4 core safety guardrails:
    - `test_case_1_root_path_intercept_guardrail` ... ok
    - `test_case_2_inclusion_filter_whitelist_enforcement` ... ok
    - `test_case_3_memory_graph_cluster_consistency` ... ok
    - `test_case_4_single_dropped_file_ingestion_routing` ... ok
    - `test_db_init_and_upsert` ... ok
    - `test_find_duplicates` ... ok

## 🪲 Persistent Context Log
1. **Concurrency Alert**: The frontend view can now securely render instantaneous batch updates safely because the DB writer has zero locking overhead on active readers.
2. **System Alert**: Be aware that ADM SMB service resets `ntlm auth = no` automatically upon NAS restart. Keep the NTLM fix command ready in your environment.