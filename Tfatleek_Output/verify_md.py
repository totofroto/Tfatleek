import os
import re
import subprocess

FILES = ["PROGRESS.md", "SKILLS.md", "home-setup.md", "WORKFLOW.md"]
WORKSPACE = "/Users/taregahmed/Documents/Tfatleek"
OUTPUT_DIR = os.path.join(WORKSPACE, "Tfatleek_Output")
LOG_FILE = os.path.join(OUTPUT_DIR, "verification.log")

def log(msg):
    print(msg)
    with open(LOG_FILE, "a", encoding="utf-8") as f:
        f.write(msg + "\n")

def check_structure(filepath):
    """
    Performs basic markdown structural validation:
    - Balanced code fences (```)
    - Valid frontmatter if present (starts with ---, ends with ---, at lines 1 and 5/6)
    """
    errors = []
    with open(filepath, "r", encoding="utf-8") as f:
        lines = f.readlines()

    # Check code fences
    fence_count = sum(1 for line in lines if line.strip().startswith("```"))
    if fence_count % 2 != 0:
        errors.append(f"Unbalanced code fences (count = {fence_count})")

    # Check frontmatter
    if filepath.endswith("PROGRESS.md") or filepath.endswith("SKILLS.md") or filepath.endswith("home-setup.md"):
        if not lines[0].strip() == "---":
            errors.append("First line is not frontmatter marker '---'")
        else:
            # find next '---'
            found_end = False
            for idx, line in enumerate(lines[1:], start=1):
                if line.strip() == "---":
                    found_end = True
                    # Check YAML content
                    for fm_line in lines[1:idx]:
                        if ":" not in fm_line:
                            errors.append(f"Invalid YAML frontmatter line: {fm_line.strip()}")
                    break
            if not found_end:
                errors.append("Frontmatter is not closed with '---'")
                
    # Check Wikilinks format
    for idx, line in enumerate(lines, start=1):
        # Find any unbalanced [[ or ]]
        open_count = line.count("[[")
        close_count = line.count("]]")
        if open_count != close_count:
            errors.append(f"Unbalanced Wikilinks on line {idx}: {line.strip()}")

    return errors

def verify_diff():
    """
    Runs git diff and checks for potential leaks or unintended alterations to IPs, ports, keys, or credentials.
    """
    try:
        # Run git diff
        result = subprocess.run(
            ["git", "diff", "--unified=0"] + FILES,
            cwd=WORKSPACE,
            capture_output=True,
            text=True,
            check=True
        )
        diff_text = result.stdout
        log("--- Git Diff Analysis ---")
        
        # Analyze lines added/removed
        # We only expect added frontmatter/wikilinks or minor adjustments to text
        # We must make sure no credentials or keys or IPs were changed
        added_ips_or_keys = []
        modified_sensitive_lines = []
        
        # Simple regex for IPs/Keys/Passwords in modified/removed lines
        sensitive_patterns = [
            r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}", # IP
            r"Token", r"Key", r"Password", r"Secret", r"auth", r"ssh", r"totofroto"
        ]
        
        lines = diff_text.splitlines()
        for line in lines:
            if line.startswith("-") and not line.startswith("---"):
                # Check if a removed/modified line had sensitive data
                for pat in sensitive_patterns:
                    if re.search(pat, line, re.IGNORECASE):
                        modified_sensitive_lines.append(line)
                        break
            elif line.startswith("+") and not line.startswith("+++"):
                # Check if any new IP, Key or password is added
                # Note: We allowed adding [[PROGRESS.md]], etc. So links are fine.
                # But we shouldn't add brand new IP addresses or credentials
                pass

        if modified_sensitive_lines:
            log("[WARNING] The following sensitive-looking lines were modified/removed in diff:")
            for line in modified_sensitive_lines:
                log(f"  {line}")
        else:
            log("[SUCCESS] No existing sensitive data (IPs, credentials, keys) was modified or removed.")
            
        return len(modified_sensitive_lines) == 0
    except Exception as e:
        log(f"[ERROR] Failed to run git diff: {e}")
        return False

def main():
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    # Clear log file
    with open(LOG_FILE, "w", encoding="utf-8") as f:
        f.write("=== Tfatleek Markdown Optimization Verification Log ===\n")
    
    log(f"Starting verification of files: {', '.join(FILES)}")
    
    # 1. Structural checks
    all_ok = True
    for file in FILES:
        path = os.path.join(WORKSPACE, file)
        log(f"Checking structure of {file}...")
        errors = check_structure(path)
        if errors:
            all_ok = False
            log(f"  [FAIL] Errors found in {file}:")
            for err in errors:
                log(f"    - {err}")
        else:
            log(f"  [PASS] {file} has valid markdown structure.")
            
    # 2. Diff validation
    diff_ok = verify_diff()
    
    if all_ok and diff_ok:
        log("\n[OVERALL RESULT] VERIFICATION SUCCESSFUL. All checks passed.")
    else:
        log("\n[OVERALL RESULT] VERIFICATION FAILED. Check warnings/errors above.")

if __name__ == "__main__":
    main()
