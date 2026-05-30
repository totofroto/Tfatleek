# 📑 TFATLEEK: PROJECT HANDOVER MANIFEST (V0.2.0)

## 🗺️ Project Architecture Summary
* **Frontend Layer:** React / Vite / TypeScript. A dark-mode desktop console utilizing native Tauri v2 event listeners for real-time IPC (Inter-Process Communication) and live UI telemetry streaming via WebSockets.
* **Systems Layer:** Rust (`src-tauri/crates/core_engine`). Multi-threaded, asynchronous file processing driven by `tokio` and `rayon`.
* **Storage & Memory:** Persistent local SQLite database core managed by a transactional `DbManager` module.

---

## 🛡️ Implemented Core Capabilities & Guardrails

1. **High-Speed Ingestion & Filtering:**
   * Utilizes a cryptographic **BLAKE3 hashing** engine to map directory manifests and isolate duplicates.
   * Enforces a strict **Document Inclusion White-list** (`pdf`, `docx`, `doc`, `txt`, `log`, `dcm`, `dicom`). All hidden macOS system files (like `.DS_Store`), compiler caches, images, and subtitle noise (`.srt`) are dropped natively in microseconds at the boundary, preventing unnecessary local LLM inference overhead.

2. **Autonomous Memory & Smart-Routing:**
   * Wired with a `file_knowledge_graph` SQLite table. The engine tracks historical file properties and categorization vectors.
   * Includes a fuzzy-name path matching algorithm. When a file is processed, the system evaluates the context against historical records and triggers a `smartCorrectionAlert` banner in the UI to notify the user of optimal path alignment.

3. **OS Drag-and-Drop Gateway:**
   * Integrated with Tauri's native `onDragDropEvent` window layer. Dropping any document directly into the viewport extracts the absolute file system path, bypasses heavy manual indexing, and kicks off an immediate Paperless-style single-file routing evaluation.

4. **Network Storage Subsystem (Asustor NAS Bridge):**
   * Integrated native `ssh2` crate pipelines. The background engine dials directly into remote storage arrays over Port 22, authenticated securely via local macOS SSH agent keys.

5. **Strict Anti-Chaos Safety Armor:**
   * **Root Intercept:** Explicit absolute path guards that instantly throw an `Err` and block execution if a user attempts to scan or mutate systemic roots like `/users/taregahmed/desktop` or `/users/taregahmed/documents`.
   * **Containment Isolation:** The backend file displacement engine is strictly prohibited from creating free folders on the desktop. All processed outputs are structurally consolidated into a single master containment directory named `Tfatleek_Output`.

---

## 📋 Active Tasks & Next Horizons
* **Next Phase A:** Further polish the interactive **Split-Pane Duplicate Mirror** card components to display cross-platform file conflicts between `💻 LOCAL MAC` and `🖥️ ASUSTOR NAS`.
* **Next Phase B:** Fine-tune the deep medical parsing maps (`.dcm` / DICOM tags) to feed structural radiological layouts into local **Gemma 4** prompts for automated anatomy classification.

---

## 🚀 Verification Status
* `cargo check` completes with zero errors or dependency limits across all internal crates (`core_engine`, `database`, `crypto`, `data_structures`).
* `npm run build` completes successfully, verifying full TypeScript type safety and production asset compilation.
