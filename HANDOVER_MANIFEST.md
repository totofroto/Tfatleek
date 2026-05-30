# 📑 TFATLEEK: PROJECT HANDOVER MANIFEST (V0.3.0)

## 🗺️ Project Architecture Summary
* **Frontend Layer:** React / Vite / TypeScript. A dark-mode desktop console (TCC) utilizing native Tauri v2 event listeners for real-time IPC and live UI telemetry streaming.
* **Systems Layer:** Rust (`src-tauri/crates/core_engine`). Multi-threaded, asynchronous file processing driven by `tokio` and `rayon`.
* **Storage & Memory:** Persistent local SQLite database managed by `DbManager`.
* **Core Server Integration:** Direct API integration with **Paperless-ngx** hosted on port **25680**.
* **AI Engine:** Local inference offloading to **Ollama** (Mac instance) for text parsing and document classification.
* **Network Transport:** Optimized for **SMB (Samba)** mounts. File detection targets natively mounted local directory systems, explicitly bypassing legacy SFTP/SSH2 logic.

---

## 🛡️ Implemented Core Capabilities & Guardrails

1. **High-Speed Ingestion & Filtering:**
   * Utilizes a cryptographic **BLAKE3 hashing** engine to map directory manifests and isolate duplicates.
   * Enforces a strict **Document Inclusion White-list** (`pdf`, `docx`, `doc`, `txt`, `log`, `dcm`, `dicom`). All hidden macOS system files (like `.DS_Store`), compiler caches, images, and subtitle noise are dropped natively at the boundary.

2. **Autonomous Memory & Smart-Routing:**
   * Wired with a `file_knowledge_graph` SQLite table. The engine tracks historical file properties and categorization vectors.
   * Includes a fuzzy-name path matching algorithm for optimal path alignment notifications.

3. **OS Drag-and-Drop Gateway:**
   * Integrated with Tauri's native `onDragDropEvent`. Dropping documents into the viewport triggers immediate Paperless-style routing evaluation using the local SMB/filesystem paths.

4. **SMB Native Bridge (Optimized):**
   * Targets natively mounted Samba shares on macOS. The system treats these as standard local directory paths, ensuring high-performance file detection without the overhead of SFTP streams. Legacy `ssh2` crates and logic are deprecated.

5. **Strict Anti-Chaos Safety Armor:**
   * **Root Intercept:** Explicit absolute path guards block execution on systemic roots like `/users/taregahmed/desktop` or `/users/taregahmed/documents`.
   * **Containment Isolation:** All processed outputs are structurally consolidated into `Tfatleek_Output`.

6. **Isolation & Security:**
   * Explicitly decoupled from obsolete stacks: Paperless-GPT, Node-RED, and Syncthing are no longer supported or present in the execution path.

---

## 📋 Active Tasks & Next Horizons
* **Phase A:** Finalize Paperless-ngx API bridge on port 25680 for automated document ingestion.
* **Phase B:** Connect local Ollama inference hooks to the classification engine for metadata enrichment.
* **Phase C:** Full deprecation and removal of `ssh2` and SFTP-related modules from `core_engine`.

---

## 🚀 Verification Status
* `cargo check` validation pass (pending confirmation of SFTP deprecation impact).
* `npm run build` completes successfully.
