import { useState, useEffect, useCallback } from "react";
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
              {files.map((file) => (
                <tr
                  key={file.id}
                  onClick={() => handleOpenFile(file.file_path)}
                  className={`hover:bg-neutral-800/40 cursor-pointer transition-colors ${
                    openingPath === file.file_path ? "bg-blue-600/10" : ""
                  }`}
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
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
