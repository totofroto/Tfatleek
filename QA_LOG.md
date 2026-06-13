---
type: project-doc
project: tfatleek
status: active
---

# QA_LOG.md — Tfatleek "Zero Bug" Verification Ledger

> **Purpose:** The evidence ledger for the full-stack hardening pass. Every module of Tfatleek
> gets a row. A module is only **✅ PROVEN** when the listed verification has actually been run
> and passed, with the command/output recorded in the Evidence column. Belief is not evidence.
>
> Gitignored (local-only). See [[HANDOFF.md]] for the active objective and [[WORKFLOW.md]] for roles.

---

## 📏 Definition of "PROVEN" (the only path to ✅)

A row may be marked ✅ PROVEN **only** if ALL of the following are true:
1. The verification method below was actually executed (not assumed).
2. It passed with zero errors and zero new warnings.
3. The exact command + a summary of its output is pasted in the Evidence column.
4. For functional behavior: at least one positive case AND one negative/edge case were exercised.

Status legend: `⬜ NOT VERIFIED` · `🟡 IN PROGRESS / PARTIAL` · `❌ BUG FOUND` · `✅ PROVEN`

**2-Strike Rule applies:** two consecutive failures with the same root error → STOP, log it in
the Escalation Log, yield to the human. No third blind attempt.

---

## 🦀 Rust Backend — `src-tauri/`

| Module / Unit | Verification method | Status | Evidence | Date |
|---|---|---|---|---|
| `cargo check` (whole workspace) | `cargo check` → 0 errors, 0 warnings | ⬜ | | |
| `cargo clippy` (lint) | `cargo clippy --all-targets -- -D warnings` | ⬜ | | |
| `cargo test` (whole workspace) | all unit/integration tests pass | ⬜ | | |
| `crypto_dedup` — BLAKE3 hashing | unit test: known input → known BLAKE3 digest | ⬜ | | |
| `crypto_dedup` — Rayon dir walker | walks a fixture tree; counts/ignores correctly | ⬜ | | |
| `database` — DbManager / SQLite | open, WAL mode on, schema present | ⬜ | | |
| `database` — `file_index` CRUD | insert/query/update round-trip | ⬜ | | |
| `database` — `file_transactions` | batch op logged + retrievable | ⬜ | | |
| `database` — `smart_groups` | 4 default groups present; id≤4 locked | ⬜ | | |
| `local_ai` — Ollama client | live `GET /api/tags` reachable on M4 | ⬜ | | |
| `local_ai` — **model string** | asserts `qwen3:14b`; NO `gemma4:e4b` anywhere | ⬜ | | |
| `core_engine/extractor.rs` | PDF, DOCX, TXT, DICOM text extraction each return content | ⬜ | | |
| `core_engine/transactions.rs` | atomic move + 2-phase commit; rollback works | ⬜ | | |
| `core_engine/transactions.rs` — Undo | `execute_system_undo` reverses a batch | ⬜ | | |
| `core_engine/settings.rs` | JSON load/save; thread-safe; env override | ⬜ | | |
| `core_engine/paperless_bridge.rs` | token from env (no literal); live API auth ok | ⬜ | | |
| **Safety: Root Intercept** | guard blocks real desktop/documents/`/` paths (confirm real home dir) | ⬜ | | |
| **Safety: File Whitelist** | only pdf/docx/doc/txt/log/dcm/dicom pass; others dropped | ⬜ | | |
| **Safety: Output Containment** | writes confined to `Tfatleek_Output/` only | ⬜ | | |
| **Safety: Confidence Gate** | move blocked when `confidence_score < 0.5` | ⬜ | | |

## ⚛️ React / TypeScript Frontend — `src/`

| Module / Unit | Verification method | Status | Evidence | Date |
|---|---|---|---|---|
| `npm run build` | builds clean, 0 TS errors | ⬜ | | |
| TypeScript typecheck | `tsc --noEmit` clean | ⬜ | | |
| `App.tsx` views | Ingestion / Scan / Batch / Settings each render | ⬜ | | |
| IPC hooks (`invoke`) | each command resolves against a live backend | ⬜ | | |
| `listen("scan-progress")` | progress events received & rendered | ⬜ | | |
| `SmartGroupsSidebar.tsx` | list/create/lock/delete behave; defaults protected | ⬜ | | |
| `SmartGroupFileList.tsx` | sort, confidence colors, tax badge, click-to-open | ⬜ | | |
| `ManifestViewer.tsx` | discovers manifests; INTACT/TAMPERED/MISSING correct; CSV export | ⬜ | | |
| `WatcherHealthDashboard.tsx` | ONLINE/STALE/OFFLINE logic; live probes; log tail colors | ⬜ | | |

## 🐍 NAS Watcher (Python 3.12) — container

| Module / Unit | Verification method | Status | Evidence | Date |
|---|---|---|---|---|
| `watcher.py` — settle/debounce | partial-write file not processed until stable | ⬜ | | |
| `watcher.py` — dedup | duplicate SHA256 stopped before classify (0 AI calls) | ⬜ | | |
| `watcher.py` — OCR-before-classify | image-only scan gets OCR layer before AI | ⬜ | | |
| `watcher.py` — **OCR layer persisted** | searchable PDF kept, not discarded (Issue #2) | ⬜ | | |
| `watcher.py` — **submit policy** | confident → taxonomy, NOT inbox-tagged (Issue #1) | ⬜ | | |
| `watcher.py` — **checksum guard** | Paperless MD5 matches inbox file before trusting OCR (Issue #5) | ⬜ | | |
| `watcher.py` — FileNotFound resilience | missing source routes to `_Unsorted`, thread survives | ⬜ | | |
| `classify_document()` — Tier 1 | Ollama qwen3:14b returns classification | ⬜ | | |
| `classify_document()` — Tier 2 | Gemini fallback triggers when Tier 1 down | ⬜ | | |
| `classify_document()` — Tier 3 | safe Paperless landing when both down | ⬜ | | |
| `rules_engine.py` — apply_rules | first-match-wins; 4 default rules; SIGHUP reload | ⬜ | | |
| `db_utils.py` — dedup helpers | hash insert/lookup persist across restart | ⬜ | | |
| `retry_worker.py` | `_pending/` retried on schedule; recovers on Paperless uptime | ⬜ | | |
| `health_beacon.py` | heartbeat written every 60s; timestamp fresh | ⬜ | | |
| supervisord | PID 1; watcher auto-restarts ≤10s after kill | ⬜ | | |
| `OCR_LANG` parameterization | env-driven, not hardcoded (Issue #3) | ⬜ | | |

## 🔗 Integration / End-to-End

| Scenario | Verification method | Status | Evidence | Date |
|---|---|---|---|---|
| Full pipeline (DE doc) | drop → settle → dedup → OCR → classify → rules → route → manifest | ⬜ | | |
| Full pipeline (Arabic doc) | OCR `ara` works; searchable in Paperless natively | ⬜ | | |
| Low-confidence path | `< 0.5` → Paperless inbox (tag 25) + NeedsReview | ⬜ | | |
| Tier failover live | stop Ollama → Gemini picks up → restore | ⬜ | | |
| Manifest integrity | tamper a file → viewer flags ❌ TAMPERED | ⬜ | | |
| Power-cycle durability | reboot M4 → Paperless data retained; mounts auto-remount | ⬜ | | |
| Container-recreate durability | recreate Paperless containers → no RAM-mount regression | ⬜ | | |

---

## 🛑 Escalation Log (2-Strike stops)

> Format: `[date] module — root error — approach 1 — approach 2 — STOPPED, awaiting human`

_(empty)_

---

## 📝 Bug Inventory (found during R0 / QA)

> Every confirmed bug gets an ID, severity, location, and fix-status. Populated as we go.

| ID | Sev | Location | Description | Status |
|----|-----|----------|-------------|--------|
| _(empty — populated after R0)_ | | | | |
