#!/usr/bin/env python3
"""
tfatleek-watcher v2 — NAS inbox monitor with AI-powered classification.

Watches /share/Papers/Tfatleek_Inbox, stabilises SMB writes, classifies
documents with qwen3:14b (via Ollama) and routes them into the taxonomy tree.
Falls back to Gemini 2.0 Flash Lite when Ollama is unreachable. Files below
the confidence threshold bypass taxonomy routing and land directly in Paperless.
"""

import os, sys, json, time, shutil, logging, threading, subprocess, tempfile
from datetime import datetime
from pathlib import Path

import requests
from watchdog.observers import Observer
from watchdog.events import FileSystemEventHandler

# ---------------------------------------------------------------------------
# Configuration — all values overridable via docker-compose environment
# ---------------------------------------------------------------------------

INBOX_PATH          = os.environ.get("TFATLEEK_INBOX",       "/share/Papers/Tfatleek_Inbox")
TAXONOMY_ROOT       = os.environ.get("TAXONOMY_ROOT",         "/share/Papers/Tfatleek")
PAPERLESS_URL       = os.environ.get("PAPERLESS_URL",         "http://192.168.254.18:25680")
PAPERLESS_TOKEN     = os.environ.get("PAPERLESS_TOKEN",       "").strip()
PAPERLESS_INBOX_TAG = int(os.environ.get("PAPERLESS_INBOX_TAG", "25"))

OLLAMA_URL          = os.environ.get("OLLAMA_URL",            "http://192.168.254.14:11434")
OLLAMA_MODEL        = os.environ.get("OLLAMA_MODEL",          "qwen3:14b")
GEMINI_API_KEY      = os.environ.get("GEMINI_API_KEY",        "")

CONFIDENCE_THRESHOLD = float(os.environ.get("CONFIDENCE_THRESHOLD", "0.5"))
SETTLE_POLLS         = int(os.environ.get("SETTLE_POLLS",     "6"))
SETTLE_INTERVAL      = float(os.environ.get("SETTLE_INTERVAL","0.5"))
SNIPPET_CHARS        = int(os.environ.get("SNIPPET_CHARS",    "2000"))

ALLOWED_EXTENSIONS = {".pdf", ".docx", ".doc", ".txt", ".log", ".dcm", ".dicom"}

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    datefmt="%Y-%m-%dT%H:%M:%S",
    handlers=[
        logging.StreamHandler(),
        logging.FileHandler("/share/Papers/Tfatleek/watcher.log", encoding="utf-8"),
    ],
)
log = logging.getLogger("tfatleek-watcher")

_processing: set[str] = set()
_lock = threading.Lock()


# ---------------------------------------------------------------------------
# File-settle debounce
# ---------------------------------------------------------------------------

def wait_for_file_stable(path: str) -> bool:
    """Poll file size until stable; guards against SMB partial-write ghost reads."""
    if not os.path.exists(path):
        return False
    last = -1
    for _ in range(SETTLE_POLLS):
        try:
            size = os.path.getsize(path)
            if size == last and size > 0:
                return True
            last = size
        except OSError:
            pass
        time.sleep(SETTLE_INTERVAL)
    return False


# ---------------------------------------------------------------------------
# Text extraction
# ---------------------------------------------------------------------------

def get_pdf_text_with_ocr_fallback(pdf_path: str) -> str:
    """Return text from a PDF, running ocrmypdf when the text layer is too sparse."""
    import pdfminer.high_level
    try:
        text = pdfminer.high_level.extract_text(pdf_path) or ""
    except Exception:
        text = ""

    if len(text.strip()) >= 100:
        log.debug("PDF text layer OK (%d chars): %s", len(text.strip()), Path(pdf_path).name)
        return text

    log.info("Sparse text (%d chars) — OCR fallback triggered for: %s",
             len(text.strip()), Path(pdf_path).name)
    tmp_path = None
    try:
        with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as tmp:
            tmp_path = tmp.name
        result = subprocess.run(
            ["ocrmypdf", "-l", "deu+eng+ara", "--skip-text", pdf_path, tmp_path],
            capture_output=True, text=True, timeout=120,
        )
        if result.returncode == 0:
            ocr_text = pdfminer.high_level.extract_text(tmp_path) or ""
            log.info("OCR complete — LEN=%d for: %s", len(ocr_text), Path(pdf_path).name)
            return ocr_text
        log.warning("ocrmypdf rc=%d for %s: %s",
                    result.returncode, Path(pdf_path).name, result.stderr[:300])
    except Exception as e:
        log.warning("OCR fallback error for %s: %s", Path(pdf_path).name, e)
    finally:
        if tmp_path:
            try:
                os.unlink(tmp_path)
            except OSError:
                pass
    return text


def extract_snippet(path: str) -> str:
    ext = Path(path).suffix.lower()
    try:
        if ext in (".txt", ".log"):
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                return f.read(SNIPPET_CHARS)
        if ext == ".pdf":
            return get_pdf_text_with_ocr_fallback(path)[:SNIPPET_CHARS]
        if ext == ".docx":
            try:
                import docx
                doc = docx.Document(path)
                return "\n".join(p.text for p in doc.paragraphs)[:SNIPPET_CHARS]
            except ImportError:
                pass
    except Exception as e:
        log.debug("Snippet extraction failed for %s: %s", path, e)
    return ""


# ---------------------------------------------------------------------------
# AI Classification
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT = (
    "You are Tfatleek's backend filing clerk. Analyze the provided file metadata and text snippets. "
    "Choose a clean, descriptive snake_case folder name for 'suggested_subfolder'. "
    "Assign a 'category' (e.g., invoices, medical, bank_statements, personal). "
    "Identify the 'correspondent' (organization or person who sent/received the document). "
    "If the file needs clarification, provide a human-readable clean file name string in 'new_clean_name'. "
    "Evaluate if the document has high semantic affinity to tax preparation or fiscal reporting and set "
    "'tax_relevant' accordingly. "
    "Also check if the content mentions any family members: Tareg Mohamed Ahmed Shek (Father), "
    "Miluda Bashir Shek (Mother), Fatima Shek (Daughter), or Sama Shek (Daughter). "
    "If a clear match is found, return their full name in 'identified_member'. "
    "Return your answer strictly within the JSON schema constraint."
)

_OLLAMA_FORMAT = {
    "type": "object",
    "properties": {
        "suggested_subfolder": {"type": "string"},
        "category":            {"type": "string"},
        "correspondent":       {"type": "string"},
        "new_clean_name":      {"type": "string"},
        "confidence_score":    {"type": "number"},
        "tax_relevant":        {"type": "boolean"},
        "reasoning":           {"type": "string"},
        "identified_member":   {"type": ["string", "null"]},
    },
    "required": ["suggested_subfolder", "category", "correspondent", "new_clean_name",
                 "confidence_score", "tax_relevant", "reasoning", "identified_member"],
}


def _size_fmt(path: str) -> str:
    b = os.path.getsize(path)
    return f"{b/1024:.1f} KB" if b < 1048576 else f"{b/1048576:.2f} MB"


def _user_content(path: str, snippet: str) -> str:
    return (
        f"File Name: {Path(path).name}\nSize: {_size_fmt(path)}\n"
        f"Snippet Content Preview: \n\"\"\"\n{snippet}\n\"\"\""
    )


def classify_ollama(path: str, snippet: str) -> dict | None:
    body = {
        "model": OLLAMA_MODEL,
        "messages": [
            {"role": "system", "content": _SYSTEM_PROMPT},
            {"role": "user",   "content": _user_content(path, snippet)},
        ],
        "stream": False,
        "format": _OLLAMA_FORMAT,
        "options": {"temperature": 0.0, "top_p": 0.1},
        "keep_alive": "30m",
    }
    try:
        resp = requests.post(f"{OLLAMA_URL.rstrip('/')}/api/chat", json=body, timeout=180)
        resp.raise_for_status()
        return json.loads(resp.json()["message"]["content"])
    except Exception as e:
        log.warning("Ollama unavailable (%s) — trying Gemini fallback.", e)
        return None


def classify_gemini(path: str, snippet: str) -> dict | None:
    if not GEMINI_API_KEY:
        return None
    prompt = (
        "You are Tfatleek's backend filing clerk. Return a JSON object with: "
        "suggested_subfolder (string, snake_case), category (string), correspondent (string), "
        "new_clean_name (string), confidence_score (number 0.0–1.0), tax_relevant (boolean), "
        "reasoning (string), identified_member (string or null).\n\n"
        "Family members: Tareg Mohamed Ahmed Shek (Father), Miluda Bashir Shek (Mother), "
        "Fatima Shek (Daughter), Sama Shek (Daughter).\n\n"
        f"{_user_content(path, snippet)}\n\nReturn ONLY the raw JSON object."
    )
    body = {
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {"response_mime_type": "application/json"},
    }
    try:
        url = (
            "https://generativelanguage.googleapis.com/v1beta/models/"
            f"gemini-2.0-flash-lite:generateContent?key={GEMINI_API_KEY}"
        )
        resp = requests.post(url, json=body, timeout=30)
        resp.raise_for_status()
        content = resp.json()["candidates"][0]["content"]["parts"][0]["text"]
        return json.loads(content)
    except Exception as e:
        log.warning("Gemini fallback failed: %s", e)
        return None


def classify(path: str) -> dict | None:
    snippet = extract_snippet(path)
    result = classify_ollama(path, snippet)
    return result if result is not None else classify_gemini(path, snippet)


# ---------------------------------------------------------------------------
# Taxonomy routing
# ---------------------------------------------------------------------------

def route_to_taxonomy(path: str, c: dict) -> str | None:
    """Move file into taxonomy tree when confidence >= threshold. Returns dest path."""
    score = c.get("confidence_score", 0.0)
    if score < CONFIDENCE_THRESHOLD:
        log.info("Confidence %.2f below %.2f — bypassing taxonomy for: %s",
                 score, CONFIDENCE_THRESHOLD, Path(path).name)
        return None

    year      = str(datetime.now().year)
    category  = (c.get("category", "uncategorized") or "uncategorized").strip().lower().replace(" ", "_")
    subfolder = (c.get("suggested_subfolder", "misc") or "misc").strip().lower().replace(" ", "_")
    orig_ext  = Path(path).suffix
    raw_name  = (c.get("new_clean_name") or "").strip()
    if raw_name and not Path(raw_name).suffix and orig_ext:
        raw_name = raw_name + orig_ext
    name      = raw_name or Path(path).name

    dest_dir = Path(TAXONOMY_ROOT) / category / subfolder / year
    dest_dir.mkdir(parents=True, exist_ok=True)

    dest = dest_dir / name
    if dest.exists():
        stem, suf = Path(name).stem, Path(name).suffix
        dest = dest_dir / f"{stem}_{int(time.time())}{suf}"

    try:
        shutil.move(path, dest)
    except FileNotFoundError:
        log.error("Source file gone before taxonomy move (race with another consumer?): %s", path)
        return None

    log.info("Routed (score=%.2f) → %s", score, dest)
    return str(dest)


# ---------------------------------------------------------------------------
# Paperless submission
# ---------------------------------------------------------------------------

def _resolve_tag_ids(headers: dict, extra_names: list[str]) -> list[str]:
    ids = [str(PAPERLESS_INBOX_TAG)]
    if not extra_names:
        return ids
    try:
        r = requests.get(f"{PAPERLESS_URL}/api/tags/?page_size=500",
                         headers=headers, timeout=15)
        if r.ok:
            for t in r.json().get("results", []):
                if any(t["name"].lower() == n.lower() for n in extra_names):
                    ids.append(str(t["id"]))
    except Exception as e:
        log.debug("Tag resolution error: %s", e)
    return ids


def submit_to_paperless(path: str, c: dict | None = None) -> bool:
    if not PAPERLESS_TOKEN:
        log.warning("PAPERLESS_TOKEN unset — skipping Paperless submission.")
        return False

    if not os.path.exists(path):
        log.error(
            "submit_to_paperless: file not found — was it moved/consumed by another process? path=%s",
            path,
        )
        return False

    headers = {"Authorization": f"Token {PAPERLESS_TOKEN}"}
    name    = Path(path).name

    extra_tags: list[str] = []
    title = name
    if c:
        if c.get("tax_relevant"):
            extra_tags.append("Tax")
        member = c.get("identified_member")
        if member:
            extra_tags.append(member)
        title = (c.get("new_clean_name") or name).strip() or name

    tag_ids = _resolve_tag_ids(headers, extra_tags)

    try:
        with open(path, "rb") as fh:
            form_data = [("title", title)] + [("tags", tid) for tid in tag_ids]
            resp = requests.post(
                f"{PAPERLESS_URL}/api/documents/post_document/",
                headers=headers,
                files={"document": (name, fh)},
                data=form_data,
                timeout=120,
            )
        if resp.status_code in (200, 202):
            log.info("Paperless accepted (HTTP %d): %s", resp.status_code, name)
            return True
        log.error("Paperless rejected %s — HTTP %d: %s", name, resp.status_code, resp.text[:200])
        return False
    except Exception as e:
        log.exception("Paperless submission failed for %s: %s", path, e)
        return False


# ---------------------------------------------------------------------------
# Processing pipeline
# ---------------------------------------------------------------------------

def process_file(path: str) -> None:
    name = Path(path).name
    log.info("Classifying: %s", name)

    c = classify(path)

    # Ollama can hold the thread for up to 90 s; re-verify the file is still present
    # before touching it (another process, a Paperless consumption watcher, or a
    # second SMB event may have consumed it in the meantime).
    if not os.path.exists(path):
        log.warning(
            "File vanished during classification (consumed by another process?): %s", name
        )
        return

    if c:
        score    = c.get("confidence_score", 0.0)
        category = c.get("category", "unknown")
        log.info("Classification — category=%s, score=%.2f, tax=%s, member=%s",
                 category, score, c.get("tax_relevant"), c.get("identified_member"))
        dest = route_to_taxonomy(path, c)
        submit_to_paperless(dest if dest else path, c)
    else:
        log.warning("Classification unavailable — direct Paperless ingest: %s", name)
        submit_to_paperless(path)


# ---------------------------------------------------------------------------
# Watchdog event handler
# ---------------------------------------------------------------------------

class InboxHandler(FileSystemEventHandler):
    def on_created(self, event):
        if not event.is_directory:
            self._handle(event.src_path)

    def on_moved(self, event):
        if not event.is_directory:
            self._handle(event.dest_path)

    def _handle(self, path: str) -> None:
        if Path(path).suffix.lower() not in ALLOWED_EXTENSIONS:
            log.debug("Ignored (not whitelisted): %s", path)
            return

        with _lock:
            if path in _processing:
                return
            _processing.add(path)

        try:
            log.info("Detected: %s — waiting for SMB settle...", Path(path).name)
            if not wait_for_file_stable(path):
                log.warning("File still changing after settle window — deferring: %s", Path(path).name)
                return
            log.info("Stable — entering v2 pipeline: %s", Path(path).name)
            process_file(path)
        finally:
            with _lock:
                _processing.discard(path)


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def _check_paperless_auth() -> None:
    """Validate Paperless token at startup; exit loudly on auth failure."""
    if not PAPERLESS_TOKEN:
        log.error("PAPERLESS_TOKEN is empty — set it in .env and run 'docker compose up -d --force-recreate'")
        sys.exit(1)
    log.info("Paperless token loaded (%d chars) — verifying connectivity…", len(PAPERLESS_TOKEN))
    try:
        r = requests.get(
            f"{PAPERLESS_URL}/api/",
            headers={"Authorization": f"Token {PAPERLESS_TOKEN}"},
            timeout=10,
        )
        if r.status_code == 401:
            log.error(
                "Paperless returned 401 Unauthorized. Token is set but rejected. "
                "Confirm the token matches what 'curl -H \"Authorization: Token <tok>\" %s/api/' returns 200 for. "
                "Then run: docker compose up -d --force-recreate",
                PAPERLESS_URL,
            )
            sys.exit(1)
        if not r.ok:
            log.warning("Paperless health-check returned HTTP %d — proceeding anyway.", r.status_code)
        else:
            log.info("Paperless auth OK (HTTP %d).", r.status_code)
    except Exception as e:
        log.warning("Paperless unreachable at startup (%s) — will retry on first document.", e)


def _warmup_ollama() -> None:
    """Send a no-op generate request so qwen3:14b is loaded before the first document arrives."""
    try:
        log.info("Warming up Ollama model %s …", OLLAMA_MODEL)
        requests.post(
            f"{OLLAMA_URL.rstrip('/')}/api/generate",
            json={"model": OLLAMA_MODEL, "prompt": "ping", "stream": False, "keep_alive": "30m"},
            timeout=240,
        )
        log.info("Ollama warmup complete.")
    except Exception as e:
        log.warning("Ollama warmup failed (%s) — will retry on first document.", e)


def main() -> None:
    inbox = Path(INBOX_PATH)
    if not inbox.exists():
        log.error("Inbox path does not exist: %s", INBOX_PATH)
        sys.exit(1)

    _check_paperless_auth()
    _warmup_ollama()

    log.info("tfatleek-watcher v2 — monitoring: %s", INBOX_PATH)
    log.info("AI: %s @ %s | confidence threshold: %.2f", OLLAMA_MODEL, OLLAMA_URL, CONFIDENCE_THRESHOLD)
    log.info("Taxonomy root: %s | Settle: %d × %.1fs", TAXONOMY_ROOT, SETTLE_POLLS, SETTLE_INTERVAL)

    handler = InboxHandler()
    observer = Observer()
    observer.schedule(handler, str(inbox), recursive=False)
    observer.start()
    try:
        while observer.is_alive():
            observer.join(timeout=5)
    except KeyboardInterrupt:
        log.info("Shutting down.")
    finally:
        observer.stop()
        observer.join()


if __name__ == "__main__":
    main()
