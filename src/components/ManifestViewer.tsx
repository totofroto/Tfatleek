import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface ManifestEntry {
  timestamp: string;
  filename: string;
  sha256: string;
  category: string;
  subfolder: string;
  correspondent: string;
  tax_relevant: boolean;
  identified_member: string;
  confidence_score: number;
  new_clean_name: string;
  ai_engine: string;
  source_path: string;
}

interface VerifyResult {
  filename: string;
  expected_hash: string;
  actual_hash: string;
  matches: boolean;
  file_exists: boolean;
}

interface TreeNode {
  label: string;
  manifestPath: string;
}

function buildTree(files: string[], nasRoot: string): TreeNode[] {
  return files.map((file) => {
    const normalized = nasRoot.endsWith("/") ? nasRoot : nasRoot + "/";
    const rel = file.startsWith(normalized) ? file.slice(normalized.length) : file;
    // Remove trailing "manifest.jsonl"
    const dir = rel.replace(/\/?manifest\.jsonl$/, "");
    const label = dir || "(root)";
    return { label, manifestPath: file };
  });
}

function formatDate(ts: string): string {
  try {
    const d = new Date(ts);
    if (isNaN(d.getTime())) return ts;
    const dd = d.getDate().toString().padStart(2, "0");
    const mm = (d.getMonth() + 1).toString().padStart(2, "0");
    const yyyy = d.getFullYear();
    const hh = d.getHours().toString().padStart(2, "0");
    const min = d.getMinutes().toString().padStart(2, "0");
    return `${dd}.${mm}.${yyyy} ${hh}:${min}`;
  } catch {
    return ts;
  }
}

export default function ManifestViewer() {
  const [nasRoot, setNasRoot] = useState<string>(
    () => localStorage.getItem("tfatleek_nas_root") || ""
  );
  const [manifestFiles, setManifestFiles] = useState<string[]>([]);
  const [selectedManifest, setSelectedManifest] = useState<string | null>(null);
  const [entries, setEntries] = useState<ManifestEntry[]>([]);
  const [loadingFiles, setLoadingFiles] = useState(false);
  const [loadingEntries, setLoadingEntries] = useState(false);
  const [verifyStates, setVerifyStates] = useState<
    Record<string, VerifyResult | "loading">
  >({});

  useEffect(() => {
    if (!nasRoot) {
      setManifestFiles([]);
      return;
    }
    setLoadingFiles(true);
    invoke<string[]>("list_manifest_files", { nasRoot })
      .then((files) => setManifestFiles(files))
      .catch((err) => console.error("Failed to list manifests:", err))
      .finally(() => setLoadingFiles(false));
  }, [nasRoot]);

  const handleBrowse = async () => {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected === "string" && selected) {
        setNasRoot(selected);
        localStorage.setItem("tfatleek_nas_root", selected);
        setSelectedManifest(null);
        setEntries([]);
        setVerifyStates({});
      }
    } catch (e) {
      console.error("Dialog error:", e);
    }
  };

  const handleSelectManifest = async (path: string) => {
    setSelectedManifest(path);
    setLoadingEntries(true);
    setVerifyStates({});
    try {
      const result = await invoke<ManifestEntry[]>("read_manifest", {
        manifestPath: path,
      });
      setEntries(result);
    } catch (e) {
      console.error("Failed to read manifest:", e);
      setEntries([]);
    } finally {
      setLoadingEntries(false);
    }
  };

  const handleVerify = async (entry: ManifestEntry) => {
    const key = `${entry.source_path}::${entry.filename}`;
    setVerifyStates((prev) => ({ ...prev, [key]: "loading" }));
    try {
      const result = await invoke<VerifyResult>("verify_manifest_entry", {
        manifestPath: entry.source_path,
        filename: entry.filename,
        expectedHash: entry.sha256,
      });
      setVerifyStates((prev) => ({ ...prev, [key]: result }));
    } catch (e) {
      console.error("Verify failed:", e);
      setVerifyStates((prev) => {
        const next = { ...prev };
        delete next[key];
        return next;
      });
    }
  };

  const handleExportCsv = async () => {
    if (!selectedManifest) return;
    try {
      const csv = await invoke<string>("export_manifest_csv", {
        manifestPath: selectedManifest,
      });
      const blob = new Blob([csv], { type: "text/csv" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      const parts = selectedManifest.replace(/\\/g, "/").split("/");
      const year = parts[parts.length - 2] || "unknown";
      const category = parts[parts.length - 4] || "manifest";
      const date = new Date().toISOString().slice(0, 10).replace(/-/g, "");
      a.href = url;
      a.download = `manifest_${category}_${year}_${date}.csv`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (e) {
      console.error("Export failed:", e);
    }
  };

  const tree = buildTree(manifestFiles, nasRoot);

  return (
    <div className="flex flex-col h-[calc(100vh-200px)]">
      {/* Header bar */}
      <div className="flex items-center gap-3 mb-4 p-4 bg-neutral-900/50 border border-neutral-800 rounded-2xl">
        <span className="text-xs font-black uppercase tracking-widest text-blue-500 whitespace-nowrap">
          Audit Trail
        </span>
        <div className="flex-1 bg-black border border-neutral-800 rounded-xl px-4 py-2 text-xs font-mono text-neutral-400 truncate min-w-0">
          {nasRoot || "No NAS root selected"}
        </div>
        <button
          onClick={handleBrowse}
          className="bg-neutral-800 hover:bg-neutral-700 border border-neutral-700 px-4 py-2 rounded-xl text-xs font-black uppercase transition-all whitespace-nowrap"
        >
          Browse
        </button>
      </div>

      {!nasRoot ? (
        <div className="flex-1 flex flex-col items-center justify-center text-neutral-600 gap-3">
          <span className="text-4xl">📂</span>
          <span className="text-sm font-mono">
            Select your NAS root folder to view audit trail
          </span>
        </div>
      ) : (
        <div className="flex flex-1 gap-4 overflow-hidden min-h-0">
          {/* Left panel: file tree */}
          <div className="w-64 flex-shrink-0 bg-neutral-950 border border-neutral-800 rounded-2xl flex flex-col overflow-hidden">
            <div className="px-4 py-3 border-b border-neutral-900 flex-shrink-0">
              <span className="text-[10px] font-black uppercase tracking-widest text-neutral-500">
                Manifest Files
              </span>
            </div>
            <div className="flex-1 overflow-y-auto">
              {loadingFiles ? (
                <div className="p-4 text-neutral-600 text-xs animate-pulse">
                  Scanning...
                </div>
              ) : manifestFiles.length === 0 ? (
                <div className="p-4 text-neutral-600 text-xs">
                  No manifest files found
                </div>
              ) : (
                <div className="py-2">
                  {tree.map((node) => (
                    <button
                      key={node.manifestPath}
                      onClick={() => handleSelectManifest(node.manifestPath)}
                      className={`w-full flex items-start gap-2 px-4 py-2.5 text-left transition-all text-[10px] font-mono ${
                        selectedManifest === node.manifestPath
                          ? "bg-blue-600/15 border-r-2 border-blue-500 text-blue-400"
                          : "text-neutral-400 hover:bg-neutral-900"
                      }`}
                    >
                      <span className="flex-shrink-0 mt-0.5">📋</span>
                      <span className="break-all leading-relaxed">{node.label}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          </div>

          {/* Right panel: entries table */}
          <div className="flex-1 bg-neutral-950 border border-neutral-800 rounded-2xl flex flex-col overflow-hidden min-w-0">
            <div className="flex items-center justify-between px-4 py-3 border-b border-neutral-900 flex-shrink-0">
              <span className="text-[10px] font-black uppercase tracking-widest text-neutral-500">
                {selectedManifest ? `Entries (${entries.length})` : "Entries"}
              </span>
              {selectedManifest && (
                <button
                  onClick={handleExportCsv}
                  className="bg-neutral-800 hover:bg-neutral-700 border border-neutral-700 px-3 py-1 rounded-lg text-[10px] font-black uppercase transition-all text-neutral-300"
                >
                  Export CSV
                </button>
              )}
            </div>

            {!selectedManifest ? (
              <div className="flex-1 flex items-center justify-center text-neutral-700 text-xs font-mono">
                Select a manifest file to view entries
              </div>
            ) : loadingEntries ? (
              <div className="flex-1 flex items-center justify-center text-neutral-600 text-xs animate-pulse">
                Loading entries...
              </div>
            ) : entries.length === 0 ? (
              <div className="flex-1 flex items-center justify-center text-neutral-700 text-xs font-mono">
                No entries in this manifest
              </div>
            ) : (
              <div className="flex-1 overflow-auto">
                <table className="w-full text-[10px] font-mono">
                  <thead className="bg-neutral-900 text-neutral-500 sticky top-0 z-10">
                    <tr>
                      <th className="p-3 text-left whitespace-nowrap">Date</th>
                      <th className="p-3 text-left">Filename</th>
                      <th className="p-3 text-left">Category</th>
                      <th className="p-3 text-left">Correspondent</th>
                      <th className="p-3 text-left">Conf</th>
                      <th className="p-3 text-left">Tax</th>
                      <th className="p-3 text-left">Engine</th>
                      <th className="p-3 text-left">Integrity</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-neutral-900">
                    {entries.map((entry, i) => {
                      const key = `${entry.source_path}::${entry.filename}`;
                      const vs = verifyStates[key];
                      return (
                        <tr key={i} className="hover:bg-neutral-900/50">
                          <td className="p-3 text-neutral-400 whitespace-nowrap">
                            {formatDate(entry.timestamp)}
                          </td>
                          <td
                            className="p-3 text-neutral-300 max-w-[150px] truncate"
                            title={entry.filename}
                          >
                            {entry.filename}
                          </td>
                          <td className="p-3 text-blue-400 whitespace-nowrap">
                            {entry.category}
                          </td>
                          <td
                            className="p-3 text-neutral-400 max-w-[100px] truncate"
                            title={entry.correspondent}
                          >
                            {entry.correspondent}
                          </td>
                          <td className="p-3 text-emerald-400 whitespace-nowrap">
                            {(entry.confidence_score * 100).toFixed(0)}%
                          </td>
                          <td className="p-3 text-center">
                            {entry.tax_relevant ? (
                              <span className="text-amber-400">✓</span>
                            ) : (
                              <span className="text-neutral-700">–</span>
                            )}
                          </td>
                          <td className="p-3 text-neutral-600 whitespace-nowrap">
                            {entry.ai_engine}
                          </td>
                          <td className="p-3 whitespace-nowrap">
                            {!vs ? (
                              <button
                                onClick={() => handleVerify(entry)}
                                className="bg-neutral-800 hover:bg-neutral-700 border border-neutral-700 px-2 py-1 rounded text-[9px] font-black uppercase transition-all"
                              >
                                Verify
                              </button>
                            ) : vs === "loading" ? (
                              <span className="text-neutral-500 text-[9px] animate-pulse">
                                Checking...
                              </span>
                            ) : !vs.file_exists ? (
                              <span className="bg-orange-900/30 text-orange-400 border border-orange-800/30 px-2 py-0.5 rounded text-[9px] font-black">
                                MISSING
                              </span>
                            ) : vs.matches ? (
                              <span className="bg-emerald-900/30 text-emerald-400 border border-emerald-800/30 px-2 py-0.5 rounded text-[9px] font-black">
                                INTACT
                              </span>
                            ) : (
                              <span
                                className="bg-red-900/30 text-red-400 border border-red-800/30 px-2 py-0.5 rounded text-[9px] font-black cursor-help"
                                title={`Expected: ${vs.expected_hash}\nActual: ${vs.actual_hash}`}
                              >
                                TAMPERED
                              </span>
                            )}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
