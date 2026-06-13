import { useState, useEffect, useCallback, Fragment } from "react";
import { invoke } from "@tauri-apps/api/core";

interface FileIndexEntry {
  id: number;
  file_path: string;
  file_name: string;
  file_size: number;
  modified_at: number;
  category: string | null;
  correspondent: string | null;
  confidence_score: number;
  tax_relevant: boolean;
  suggested_subfolder: string | null;
  identified_member: string | null;
  monetary_amount: string | null;
  document_date: string | null;
}

interface Props {
  groupId: number;
  groupName: string;
  groupIcon: string;
}

function fileExtIcon(filename: string): string {
  const ext = filename.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    pdf: "📄",
    docx: "📝",
    doc: "📝",
    txt: "📃",
    log: "📋",
    dcm: "🩻",
    dicom: "🩻",
  };
  return map[ext] ?? "📄";
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

function yearFromTs(ts: number): string {
  return new Date(ts * 1000).getFullYear().toString();
}

function confidenceColor(score: number): string {
  if (score >= 0.8) return "text-emerald-400";
  if (score >= 0.5) return "text-amber-400";
  return "text-red-400";
}

export default function SmartGroupFileList({ groupId, groupName, groupIcon }: Props) {
  const [files, setFiles] = useState<FileIndexEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [openingPath, setOpeningPath] = useState<string | null>(null);

  const [selectedFileId, setSelectedFileId] = useState<number | null>(null);
  const [relatedFiles, setRelatedFiles] = useState<FileIndexEntry[]>([]);
  const [loadingRelated, setLoadingRelated] = useState(false);

  const loadFiles = useCallback(async () => {
    setLoading(true);
    try {
      const data = await invoke<FileIndexEntry[]>("get_smart_group_files", { groupId });
      setFiles(data);
    } catch (e) {
      console.error("Failed to load group files:", e);
      setFiles([]);
    } finally {
      setLoading(false);
    }
  }, [groupId]);

  useEffect(() => {
    loadFiles();
  }, [loadFiles]);

  useEffect(() => {
    setSelectedFileId(null);
    setRelatedFiles([]);
  }, [groupId]);

  const handleOpenFile = async (filePath: string) => {
    setOpeningPath(filePath);
    try {
      await invoke("open_file", { path: filePath });
    } catch (e) {
      console.error("Failed to open file:", e);
    } finally {
      setTimeout(() => setOpeningPath(null), 800);
    }
  };

  const handleSelectFile = async (file: FileIndexEntry) => {
    if (selectedFileId === file.id) {
      setSelectedFileId(null);
      setRelatedFiles([]);
      return;
    }

    setSelectedFileId(file.id);
    setLoadingRelated(true);
    setRelatedFiles([]);

    try {
      const data = await invoke<FileIndexEntry[]>("get_related_documents", {
        fileId: file.id.toString(),
      });
      setRelatedFiles(data);
    } catch (e) {
      console.error("Failed to load related documents:", e);
      setRelatedFiles([]);
    } finally {
      setLoadingRelated(false);
    }
  };

  return (
    <div className="flex flex-col flex-1 min-w-0 overflow-hidden">
      {/* Header */}
      <div className="flex items-center gap-3 px-6 py-4 border-b border-neutral-900">
        <span className="text-xl">{groupIcon}</span>
        <h2 className="text-sm font-black uppercase tracking-widest text-neutral-200">
          {groupName}
        </h2>
        {!loading && (
          <span className="text-[10px] bg-neutral-900 border border-neutral-800 text-neutral-500 px-2 py-0.5 rounded-full font-mono">
            {files.length} document{files.length !== 1 ? "s" : ""}
          </span>
        )}
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center h-48">
            <div className="text-[10px] font-black uppercase tracking-widest text-neutral-600 animate-pulse">
              Loading...
            </div>
          </div>
        ) : files.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-48 gap-3">
            <span className="text-4xl opacity-30">🗂️</span>
            <p className="text-[11px] font-black uppercase tracking-widest text-neutral-600">
              No documents match this group
            </p>
          </div>
        ) : (
          <table className="w-full text-[11px] font-mono">
            <thead className="bg-neutral-900/80 text-neutral-500 sticky top-0">
              <tr>
                <th className="px-4 py-3 text-left font-black uppercase tracking-widest text-[9px] w-8"></th>
                <th className="px-4 py-3 text-left font-black uppercase tracking-widest text-[9px]">
                  Filename
                </th>
                <th className="px-4 py-3 text-left font-black uppercase tracking-widest text-[9px]">
                  Category
                </th>
                <th className="px-4 py-3 text-left font-black uppercase tracking-widest text-[9px]">
                  Correspondent
                </th>
                <th className="px-4 py-3 text-center font-black uppercase tracking-widest text-[9px]">
                  Year
                </th>
                <th className="px-4 py-3 text-center font-black uppercase tracking-widest text-[9px]">
                  Conf.
                </th>
                <th className="px-4 py-3 text-center font-black uppercase tracking-widest text-[9px]">
                  Tax
                </th>
                <th className="px-4 py-3 text-right font-black uppercase tracking-widest text-[9px]">
                  Size
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-neutral-900/50">
              {files.map((file) => {
                const isExpanded = selectedFileId === file.id;
                return (
                  <Fragment key={file.id}>
                    <tr
                      onClick={() => handleSelectFile(file)}
                      onDoubleClick={() => handleOpenFile(file.file_path)}
                      className={`hover:bg-neutral-800/40 cursor-pointer transition-colors ${
                        openingPath === file.file_path ? "bg-blue-600/10" : ""
                      } ${isExpanded ? "bg-neutral-800/60" : ""}`}
                      title={file.file_path}
                    >
                      <td className="px-4 py-2.5 text-center text-base leading-none">
                        {fileExtIcon(file.file_name)}
                      </td>
                      <td className="px-4 py-2.5 text-neutral-200 max-w-xs truncate">
                        {file.file_name}
                      </td>
                      <td className="px-4 py-2.5 text-neutral-500 truncate max-w-[120px]">
                        {file.category ?? (
                          <span className="text-neutral-700 italic">—</span>
                        )}
                      </td>
                      <td className="px-4 py-2.5 text-neutral-500 truncate max-w-[120px]">
                        {file.correspondent ?? (
                          <span className="text-neutral-700 italic">—</span>
                        )}
                      </td>
                      <td className="px-4 py-2.5 text-center text-neutral-500">
                        {yearFromTs(file.modified_at)}
                      </td>
                      <td className={`px-4 py-2.5 text-center font-bold ${confidenceColor(file.confidence_score)}`}>
                        {(file.confidence_score * 100).toFixed(0)}%
                      </td>
                      <td className="px-4 py-2.5 text-center">
                        {file.tax_relevant ? (
                          <span title="Tax relevant">💰</span>
                        ) : (
                          <span className="text-neutral-800">—</span>
                        )}
                      </td>
                      <td className="px-4 py-2.5 text-right text-neutral-600">
                        {formatBytes(file.file_size)}
                      </td>
                    </tr>
                    {isExpanded && (
                      <tr key={`${file.id}-expanded`}>
                        <td colSpan={8} className="bg-neutral-950/80 px-6 py-4 border-y border-neutral-900">
                          <div className="flex flex-col gap-4">
                            {/* Detail Panel Header */}
                            <div className="flex items-start justify-between">
                              <div className="flex flex-col gap-1">
                                <span className="text-[10px] font-black uppercase tracking-widest text-neutral-500">
                                  Document Detail Inspector
                                </span>
                                <span className="text-[12px] font-bold text-neutral-200 truncate max-w-2xl">
                                  {file.file_name}
                                </span>
                                <span className="text-[10px] text-neutral-600 font-mono select-all">
                                  {file.file_path}
                                </span>
                              </div>
                              <div className="flex gap-2">
                                <button
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    handleOpenFile(file.file_path);
                                  }}
                                  disabled={openingPath === file.file_path}
                                  className={`px-3 py-1.5 text-[10px] font-black uppercase tracking-wider rounded border transition-all ${
                                    openingPath === file.file_path
                                      ? "bg-neutral-900 border-neutral-800 text-neutral-600 cursor-not-allowed"
                                      : "bg-neutral-900 hover:bg-neutral-800 border-neutral-800 hover:border-neutral-700 text-neutral-300"
                                  }`}
                                >
                                  {openingPath === file.file_path ? "Opening..." : "Open Document"}
                                </button>
                              </div>
                            </div>

                            {/* Info Grid */}
                            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 bg-neutral-900/30 p-3 rounded border border-neutral-900/50">
                              <div>
                                <div className="text-[9px] font-black uppercase tracking-widest text-neutral-600 mb-0.5">
                                  Category
                                </div>
                                <div className="text-neutral-300 font-bold">
                                  {file.category ?? <span className="text-neutral-700 italic">None</span>}
                                </div>
                              </div>
                              <div>
                                <div className="text-[9px] font-black uppercase tracking-widest text-neutral-600 mb-0.5">
                                  Correspondent
                                </div>
                                <div className="text-neutral-300 font-bold">
                                  {file.correspondent ?? <span className="text-neutral-700 italic">None</span>}
                                </div>
                              </div>
                              <div>
                                <div className="text-[9px] font-black uppercase tracking-widest text-neutral-600 mb-0.5">
                                  Monetary Amount
                                </div>
                                <div className="text-emerald-400 font-bold">
                                  {file.monetary_amount ?? <span className="text-neutral-700 italic">—</span>}
                                </div>
                              </div>
                              <div>
                                <div className="text-[9px] font-black uppercase tracking-widest text-neutral-600 mb-0.5">
                                  Document Date
                                </div>
                                <div className="text-neutral-300 font-bold">
                                  {file.document_date ?? <span className="text-neutral-700 italic">—</span>}
                                </div>
                              </div>
                            </div>

                            {/* Related Documents Section */}
                            <div className="flex flex-col gap-2">
                              <div className="flex items-center gap-2">
                                <span className="text-[10px] font-black uppercase tracking-widest text-neutral-400">
                                  Related Documents
                                </span>
                                {!loadingRelated && (
                                  <span className="text-[9px] bg-neutral-900 border border-neutral-800 text-neutral-500 px-1.5 py-0.2 rounded font-mono">
                                    {relatedFiles.length} match{relatedFiles.length !== 1 ? "es" : ""}
                                  </span>
                                )}
                              </div>

                              {loadingRelated ? (
                                <div className="text-[10px] font-black uppercase tracking-widest text-neutral-600 animate-pulse py-2">
                                  Querying Knowledge Graph...
                                </div>
                              ) : relatedFiles.length === 0 ? (
                                <div className="text-[10px] text-neutral-600 italic py-1 bg-neutral-900/10 border border-dashed border-neutral-900/50 rounded px-3">
                                  No related documents found sharing the correspondent "{file.correspondent ?? 'N/A'}" or monetary amount "{file.monetary_amount ?? 'N/A'}".
                                </div>
                              ) : (
                                <div className="flex flex-col gap-1.5 max-h-48 overflow-y-auto">
                                  <div className="text-[10px] text-neutral-500 mb-1">
                                    {relatedFiles.length} related document{relatedFiles.length !== 1 ? "s" : ""} found for{" "}
                                    <span className="text-neutral-300 font-bold">
                                      {file.correspondent || file.monetary_amount || "this document"}
                                    </span>:
                                  </div>
                                  
                                  <div className="grid grid-cols-1 gap-1">
                                    {relatedFiles.map((rel) => (
                                      <div
                                        key={rel.id}
                                        onClick={(e) => {
                                          e.stopPropagation();
                                          handleOpenFile(rel.file_path);
                                        }}
                                        className="flex items-center justify-between p-2 rounded bg-neutral-900/50 hover:bg-neutral-900 border border-neutral-900 hover:border-neutral-800 cursor-pointer transition-all group"
                                      >
                                        <div className="flex items-center gap-2 min-w-0">
                                          <span className="text-sm">{fileExtIcon(rel.file_name)}</span>
                                          <span className="text-neutral-300 font-bold truncate group-hover:text-blue-400 transition-colors">
                                            {rel.file_name}
                                          </span>
                                          {rel.category && (
                                            <span className="text-[9px] bg-neutral-900 border border-neutral-800/80 text-neutral-500 px-1.5 py-0.2 rounded font-mono">
                                              {rel.category}
                                            </span>
                                          )}
                                        </div>
                                        <div className="flex items-center gap-3 text-neutral-500 text-[10px] shrink-0 font-mono">
                                          {rel.monetary_amount && (
                                            <span className="text-emerald-500 font-bold">{rel.monetary_amount}</span>
                                          )}
                                          {rel.document_date && (
                                            <span>{rel.document_date}</span>
                                          )}
                                          <span>({formatBytes(rel.file_size)})</span>
                                        </div>
                                      </div>
                                    ))}
                                  </div>
                                </div>
                              )}
                            </div>
                          </div>
                        </td>
                      </tr>
                    )}
                  </Fragment>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
