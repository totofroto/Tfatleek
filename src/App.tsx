import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface ScanResult {
  file_path: string;
  file_name: string;
  file_size: number;
  modified_at: number;
  partial_hash: string | null;
  full_hash: string | null;
}

interface AiClassificationResult {
  suggested_subfolder: string;
  new_clean_name: string;
  confidence_score: number;
  reasoning: string;
}

interface ProgressPayload {
  current: number;
  total: number;
  percentage: number;
  current_file: string;
}

interface IngestionManifest {
  originalPath: string;
  suggestedName: string;
  identifiedCategory: string;
  detectedDate: string; // YYYY-MM-DD
  suggestedTargetTree: string;
  isTaxRelevant: boolean;
}

export default function App() {
  const [activeTab, setActiveTab] = useState<'dashboard' | 'settings'>('dashboard');
  const [excludedExtensions, setExcludedExtensions] = useState<string[]>(['srt', 'vtt', 'pyc', 'jsn', 'gz', 'html', 'png', 'jpg', 'gif']);
  const [targetPath, setTargetPath] = useState("");
  const [status, setStatus] = useState<"idle" | "scanning" | "success" | "error" | "batching">("idle");
  const [results, setResults] = useState<ScanResult[]>([]);
  const [showDuplicatesOnly, setShowDuplicatesOnly] = useState(false);
  const [duplicateGroups, setDuplicateGroups] = useState<any[]>([]);
  const [errorMessage, setErrorMessage] = useState("");
  
  const [selectedFile, setSelectedFile] = useState<ScanResult | null>(null);
  const [aiResult, setAiResult] = useState<AiClassificationResult | null>(null);
  const [isAiLoading, setIsAiLoading] = useState(false);
  const [undoStatus, setUndoStatus] = useState<string | null>(null);
  const [batchProgress, setBatchProgress] = useState<ProgressPayload | null>(null);
  const [newExtension, setNewExtension] = useState("");

  const [isDragging, setIsDragging] = useState(false);
  const [droppedFileLog, setDroppedFileLog] = useState<string | null>(null);
  const [smartCorrectionAlert, setSmartCorrectionAlert] = useState<string | null>(null);

  const [pendingManifest, setPendingManifest] = useState<IngestionManifest | null>(null);
  const [selectedStorageTier, setSelectedStorageTier] = useState<"LOCAL" | "NAS">("LOCAL");

  useEffect(() => {
    const unlistenFileDrop = getCurrentWindow().onDragDropEvent((event) => {
      if (event.payload.type === 'enter') {
        setIsDragging(true);
      } else if (event.payload.type === 'leave') {
        setIsDragging(false);
      } else if (event.payload.type === 'drop') {
        setIsDragging(false);
        
        // Grab the absolute OS file path of the dropped item
        const droppedPaths = event.payload.paths;
        if (droppedPaths.length > 0) {
          const targetPath = droppedPaths[0];
          
          // Enforce our non-negotiable desktop safety check
          const lowerPath = targetPath.toLowerCase();
          if (lowerPath === "/users/taregahmed/desktop" || lowerPath === "/users/taregahmed/documents") {
            alert("Safety Guardrail: Cannot drop direct root directories. Please drop specific files or subfolders.");
            return;
          }

          // Trigger immediate Paperless-style ingestion processing
          handleDroppedIngestion(targetPath);
        }
      }
    });

    return () => {
      unlistenFileDrop.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    const unlistenPromise = listen<ProgressPayload>("scan-progress", (event) => {
      setBatchProgress(event.payload);
    });

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  const handleDroppedIngestion = async (filePath: string) => {
    // Extract just the file name for the loading text
    const fileName = filePath.split('/').pop() || filePath;
    setDroppedFileLog(`Analyzing: ${fileName}...`);
    setSmartCorrectionAlert(null);
    
    try {
      // When a path is analyzed via handleDroppedIngestion:
      const memoryRecommendation = await invoke<string | null>("query_contextual_memory_match", { incomingPath: filePath });
      if (memoryRecommendation) {
        setSmartCorrectionAlert(`💡 Optimization Notice: I remember you usually save these files in your NAS or subfolder cluster. Let's correct its path assignment.`);
      }

      // Call the optimized backend module to classify the file
      const resultJson = await invoke<string>("process_single_dropped_file", { path: filePath });
      const manifest: IngestionManifest = JSON.parse(resultJson);
      
      setPendingManifest(manifest);
      setDroppedFileLog(`✓ Analysis Complete: ${fileName}`);
    } catch (error) {
      setDroppedFileLog(`❌ Error routing file: ${error}`);
    }
  };

  const handleExecuteFinalCommit = async () => {
    if (!pendingManifest) return;

    try {
      const result = await invoke<string>("execute_relocation_commit", {
        originalPath: pendingManifest.originalPath,
        suggestedName: pendingManifest.suggestedName,
        identifiedCategory: pendingManifest.identifiedCategory,
        detectedDate: pendingManifest.detectedDate,
        storageTier: selectedStorageTier,
        isTaxRelevant: pendingManifest.isTaxRelevant
      });
      
      setDroppedFileLog(`✓ ${result}`);
      setPendingManifest(null);
      
      // Refresh results if we are on dashboard
      if (activeTab === 'dashboard' && status === 'success') {
        handleScan();
      }
    } catch (error) {
      setErrorMessage(`Commit Error: ${error}`);
      setPendingManifest(null);
    }
  };

  const handleScan = async () => {
    if (!targetPath.trim()) {
      setErrorMessage("Please enter a valid directory path.");
      setStatus("error");
      return;
    }

    setStatus("scanning");
    setErrorMessage("");
    setSelectedFile(null);
    setAiResult(null);

    try {
      // Invoke our background tokio worker via Tauri IPC
      const rawJsonResponse = await invoke<string>("start_dedup_scan", { targetPath });
      const parsedResults: ScanResult[] = JSON.parse(rawJsonResponse);
      
      setResults(parsedResults);
      setStatus("success");
    } catch (error) {
      setErrorMessage(String(error));
      setStatus("error");
    }
  };

  const handleAiAnalyze = async () => {
    if (!selectedFile) return;

    setIsAiLoading(true);
    setAiResult(null);
    try {
      const response = await invoke<string>("classify_file_with_ai", { filePath: selectedFile.file_path });
      setAiResult(JSON.parse(response));
    } catch (error) {
      setErrorMessage(`AI Orchestration Error: ${error}`);
    } finally {
      setIsAiLoading(false);
    }
  };

  const handleBatchOrganize = async () => {
    if (!targetPath.trim()) return;
    
    setStatus("batching");
    setBatchProgress(null);
    
    try {
      const result = await invoke<string>("trigger_batch_ai_organization", { 
        targetPath,
        customExcludes: excludedExtensions
      });
      console.log(result);
      setStatus("success");
      // Re-scan to update the UI with new locations
      handleScan();
    } catch (error) {
      setErrorMessage(`Batch Error: ${error}`);
      setStatus("error");
    }
  };

  const handleUndo = async () => {
    try {
      const result = await invoke<string>("trigger_system_undo");
      setUndoStatus(result);
      // Clear status after 5 seconds
      setTimeout(() => setUndoStatus(null), 5000);
      // Re-scan to update the UI
      if (status === "success") handleScan();
    } catch (error) {
      setErrorMessage(`Undo Error: ${error}`);
    }
  };

  const fetchIsolatedDuplicates = async () => {
    try {
      const response = await invoke<string>("fetch_isolated_duplicates");
      const rawGroups: [string, number, string][] = JSON.parse(response);
      
      const transformedGroups = rawGroups.map(([hash, size, pathsStr]) => ({
        hash,
        fileSize: formatBytes(size),
        paths: pathsStr.split('|').map(path => ({
          path,
          name: path.split('/').pop() || path
        }))
      }));

      setDuplicateGroups(transformedGroups);
      setShowDuplicatesOnly(true);
    } catch (error) {
      setErrorMessage(`Fetch Duplicates Error: ${error}`);
    }
  };

  const handleDeleteClick = async (filePath: string) => {
    if (!confirm(`Are you sure you want to permanently delete this file?\n${filePath}`)) return;
    
    try {
      const result = await invoke<string>("execute_file_deletion", { filePath });
      console.log(result);
      // Refresh views
      if (showDuplicatesOnly) {
        fetchIsolatedDuplicates();
      } else {
        handleScan();
      }
    } catch (error) {
      setErrorMessage(`Deletion Error: ${error}`);
    }
  };

  // Helper utility to format byte arrays to human-readable strings
  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return "0 Bytes";
    const k = 1024;
    const sizes = ["Bytes", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  };

  return (
    <main className="min-h-screen bg-neutral-950 text-neutral-100 p-8 font-sans">
      {pendingManifest && (
        <div className="fixed inset-0 bg-black/80 backdrop-blur-md flex items-center justify-center p-4 z-50 animate-fade-in">
          <div className="bg-neutral-950 border border-neutral-800 rounded-xl p-6 max-w-xl w-full shadow-2xl font-mono">
            <div className="text-sm font-bold text-emerald-400 border-b border-neutral-950 pb-3 mb-4 uppercase tracking-wider">
              🔎 Ingestion Gatekeeper Manifest Verification
            </div>
            
            <div className="space-y-4 text-xs">
              <div>
                <span className="text-neutral-500 block mb-1">IDENTIFIED FILE TYPE / TARGET:</span>
                <span className="bg-neutral-900 border border-neutral-800 px-2 py-1 rounded text-neutral-200 uppercase font-bold">
                  📁 {pendingManifest.identifiedCategory}
                </span>
              </div>

              <div>
                <span className="text-neutral-500 block mb-1">PROPOSED OPTIMIZED FILENAME:</span>
                <input 
                  type="text" 
                  value={pendingManifest.suggestedName}
                  onChange={(e) => setPendingManifest({...pendingManifest, suggestedName: e.target.value})}
                  className="w-full bg-black border border-neutral-800 px-3 py-2 rounded text-amber-400 focus:border-amber-500 outline-none"
                />
              </div>

              <div>
                <span className="text-neutral-500 block mb-1">CHOOSE STORAGE TARGET TIER:</span>
                <div className="grid grid-cols-2 gap-3">
                  <button 
                    onClick={() => setSelectedStorageTier("LOCAL")}
                    className={`py-2 rounded border font-bold transition-all ${selectedStorageTier === "LOCAL" ? "bg-emerald-950 text-emerald-400 border-emerald-600" : "bg-black text-neutral-500 border-neutral-900"}`}
                  >
                    💻 LOCAL MAC SSD
                  </button>
                  <button 
                    onClick={() => setSelectedStorageTier("NAS")}
                    className={`py-2 rounded border font-bold transition-all ${selectedStorageTier === "NAS" ? "bg-blue-950 text-blue-400 border-blue-600" : "bg-black text-neutral-500 border-neutral-900"}`}
                  >
                    🖥️ ASUSTOR NAS OVER SFTP
                  </button>
                </div>
              </div>

              {pendingManifest.isTaxRelevant && (
                <div className="mt-2 p-2 bg-amber-950/40 border border-amber-900/60 rounded-md text-[11px] font-mono text-amber-400 flex items-center justify-between">
                  <span>💼 Automatische Steuererklaerung-Kopie</span>
                  <span className="text-[9px] bg-amber-900 text-amber-200 px-1.5 py-0.5 rounded uppercase font-bold">DUAL ROUTING ACTIVE</span>
                </div>
              )}
            </div>

            <div className="mt-6 pt-4 border-t border-neutral-900 flex justify-end space-x-3">
              <button 
                onClick={() => setPendingManifest(null)}
                className="px-4 py-2 bg-neutral-900 hover:bg-neutral-850 text-neutral-400 text-xs rounded transition-all uppercase"
              >
                Cancel Ingestion
              </button>
              <button 
                onClick={handleExecuteFinalCommit}
                className="px-4 py-2 bg-emerald-600 hover:bg-emerald-500 text-black text-xs font-bold rounded transition-all uppercase shadow-lg shadow-emerald-950/40"
              >
                Confirm & Execute Relocation ⚡
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Header Profile Section */}
      <header className="mb-12 border-b border-neutral-900 pb-6 max-w-6xl mx-auto flex justify-between items-end">
        <div>
          <h1 className="text-3xl font-light tracking-widest uppercase">
            Tfatleek <span className="text-blue-500 font-medium">v0.1.0</span>
          </h1>
          <div className="flex gap-6 mt-4">
            <button 
              onClick={() => setActiveTab('dashboard')}
              className={`text-xs font-mono uppercase tracking-widest pb-1 border-b-2 transition-all ${activeTab === 'dashboard' ? 'text-blue-500 border-blue-500' : 'text-neutral-500 border-transparent hover:text-neutral-300'}`}
            >
              Dashboard
            </button>
            <button 
              onClick={() => setActiveTab('settings')}
              className={`text-xs font-mono uppercase tracking-widest pb-1 border-b-2 transition-all ${activeTab === 'settings' ? 'text-blue-500 border-blue-500' : 'text-neutral-500 border-transparent hover:text-neutral-300'}`}
            >
              Exclusion Settings
            </button>
          </div>
          <p className="mt-4 text-neutral-500 font-mono text-xs">// Local Storage & NAS Deduplication Architecture</p>
        </div>
        <div className="flex items-center gap-4">
          {undoStatus && (
            <span className="text-emerald-400 font-mono text-xs animate-pulse">
              [UNDO]: {undoStatus}
            </span>
          )}
          <button 
            onClick={handleUndo}
            className="text-xs font-mono text-neutral-400 bg-neutral-900 hover:bg-neutral-800 px-3 py-1 rounded border border-neutral-800 transition-colors cursor-pointer"
          >
            Undo Last Action
          </button>
          <div className="font-mono text-xs text-neutral-400 bg-neutral-900 px-3 py-1 rounded border border-neutral-800">
            Hardware: Apple Silicon M1 Pro Profile
          </div>
        </div>
      </header>

      <div className="max-w-6xl mx-auto space-y-8">
        {activeTab === 'dashboard' ? (
          <>
            {/* Native OS Drag & Drop Ingestion Panel */}
            <div className={`mt-4 border-2 border-dashed rounded-xl p-8 transition-all duration-200 text-center flex flex-col items-center justify-center ${
              isDragging 
                ? 'border-blue-500 bg-blue-950/20 text-blue-400 scale-[1.01]' 
                : 'border-neutral-800 bg-neutral-950/40 text-neutral-400 hover:border-neutral-700'
            }`}>
              <div className="text-3xl mb-2">📥</div>
              <div className="font-mono text-xs uppercase tracking-wider font-bold">
                Paperless Ingestion Gateway
              </div>
              <div className="text-[11px] font-mono text-neutral-500 mt-1">
                Drag & Drop any PDF, Word Document, or Medical DICOM file directly here to route instantly
              </div>
              
              {droppedFileLog && (
                <div className="mt-3 px-3 py-1 bg-black border border-neutral-900 rounded text-[10px] font-mono text-amber-400 animate-pulse">
                  ⚡ Status: {droppedFileLog}
                </div>
              )}

              {smartCorrectionAlert && (
                <div className="mt-3 p-3 bg-amber-950/30 border border-amber-900/60 rounded-lg text-left flex flex-col space-y-1 animate-fade-in">
                  <div className="text-[11px] font-mono font-bold text-amber-400 uppercase tracking-wide">
                    🤖 Active Memory Correction Prompt
                  </div>
                  <div className="text-[10px] font-mono text-neutral-300">
                    {smartCorrectionAlert}
                  </div>
                </div>
              )}
            </div>

            {/* Input Console Control box */}
            <section className="bg-neutral-900/40 border border-neutral-900 p-6 rounded-xl space-y-4">
              <h2 className="text-sm font-mono text-neutral-400 uppercase tracking-wider">// Control Console</h2>
              <div className="flex gap-4">
                <input
                  type="text"
                  value={targetPath}
                  onChange={(e) => setTargetPath(e.target.value)}
                  placeholder="e.g., /Users/username/Desktop or /Volumes/Lockerstor"
                  className="flex-1 bg-neutral-950 border border-neutral-800 rounded-lg px-4 py-2.5 font-mono text-sm focus:outline-none focus:border-blue-500 text-neutral-200"
                  disabled={status === "scanning" || status === "batching"}
                />
                <button
                  onClick={handleScan}
                  disabled={status === "scanning" || status === "batching"}
                  className="bg-blue-600 hover:bg-blue-500 disabled:bg-neutral-800 text-white font-mono text-sm px-6 py-2.5 rounded-lg font-medium tracking-wide transition-colors duration-150 shadow-md shadow-blue-950/20 cursor-pointer disabled:cursor-not-allowed"
                >
                  {status === "scanning" ? "Processing Engine Active..." : "Trigger Scan"}
                </button>
                {results.length > 0 && (
                  <button
                    onClick={handleBatchOrganize}
                    disabled={status === "scanning" || status === "batching"}
                    className="bg-emerald-600 hover:bg-emerald-500 disabled:bg-neutral-800 text-white font-mono text-sm px-6 py-2.5 rounded-lg font-medium tracking-wide transition-colors duration-150 shadow-md shadow-emerald-950/20 cursor-pointer disabled:cursor-not-allowed"
                  >
                    {status === "batching" ? "AI Batching Active..." : "Automated Batch Organize"}
                  </button>
                )}
              </div>
            </section>

            {/* Dynamic Status / Feedback Logs */}
            {status === "scanning" && (
              <div className="p-12 text-center border border-dashed border-neutral-800 rounded-xl space-y-3">
                <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-blue-500 mx-auto"></div>
                <p className="text-sm font-mono text-neutral-400 animate-pulse">Scanning file tree hierarchy. Computing size tiers and BLAKE3 blocks...</p>
              </div>
            )}

            {status === "batching" && batchProgress && (
              <div className="p-8 border border-neutral-800 rounded-xl space-y-6 bg-neutral-900/20 animate-fade-in">
                <div className="flex justify-between items-end">
                  <div className="space-y-1">
                    <h3 className="text-xs font-mono text-neutral-400 uppercase tracking-widest">// Batch Progress Engine</h3>
                    <p className="text-sm font-medium text-neutral-200">Processing: {batchProgress.current_file}</p>
                  </div>
                  <p className="text-xs font-mono text-neutral-500">{batchProgress.current} / {batchProgress.total} Files</p>
                </div>
                
                <div className="w-full bg-neutral-900 h-2 rounded-full overflow-hidden border border-neutral-800">
                  <div 
                    className="bg-blue-500 h-full transition-all duration-300 ease-out"
                    style={{ width: `${batchProgress.percentage}%` }}
                  />
                </div>
                
                <div className="flex justify-between items-center">
                  <p className="text-[10px] font-mono text-neutral-500 uppercase tracking-tighter animate-pulse">Gemma 4 executing classification inference...</p>
                  <p className="text-xs font-bold font-mono text-blue-400">{batchProgress.percentage.toFixed(1)}%</p>
                </div>
              </div>
            )}

            {status === "error" && (
              <div className="bg-red-950/20 border border-red-900/50 text-red-400 px-4 py-3 rounded-lg font-mono text-sm">
                [SYSTEM ERROR]: {errorMessage}
              </div>
            )}

            {/* Results Metrics Data View */}
            {status === "success" && (
              <div className="flex flex-col lg:flex-row gap-6 items-start">
                {/* Left Side: The Scan Manifest Table (Takes up 70% width if file selected) */}
                <div className={`w-full transition-all duration-300 ${selectedFile ? 'lg:w-2/3' : 'lg:w-full'}`}>
                  <section className="space-y-4 animate-fade-in min-w-0">
                    <div className="flex justify-between items-center">
                      <h3 className="text-md font-mono text-neutral-400 uppercase tracking-wider">// Scan Manifest ({showDuplicatesOnly ? duplicateGroups.length : results.length} {showDuplicatesOnly ? "Groups" : "Files"} Discovered)</h3>
                      <div className="flex gap-2">
                        <button
                          onClick={() => {
                            if (showDuplicatesOnly) {
                              setShowDuplicatesOnly(false);
                            } else {
                              fetchIsolatedDuplicates();
                            }
                          }}
                          className={`text-xs font-mono px-3 py-1 rounded border transition-colors cursor-pointer ${showDuplicatesOnly ? 'bg-blue-600 border-blue-500 text-white' : 'bg-neutral-900 border-neutral-800 text-neutral-400 hover:bg-neutral-800'}`}
                        >
                          {showDuplicatesOnly ? "[X] Duplicates Only" : "Show Duplicates Only"}
                        </button>
                        <span className="text-xs bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 px-2 py-0.5 rounded-full font-mono">
                          State: Database Synchronized
                        </span>
                      </div>
                    </div>

                    {showDuplicatesOnly ? (
                      <div className="mt-6 space-y-4">
                        <div className="text-[11px] font-mono text-neutral-400 uppercase tracking-wider mb-2">
                          ⚠️ Cryptographic Duplicate Groups Isolated
                        </div>
                        
                        {/* Map through your grouped duplicate hash states */}
                        {duplicateGroups.map((group, groupIdx) => (
                          <div key={groupIdx} className="bg-neutral-950 border border-neutral-850 rounded-lg p-4 shadow-xl">
                            <div className="flex justify-between items-center border-b border-neutral-900 pb-2 mb-3">
                              <span className="font-mono text-[10px] text-neutral-500">HASH: <span className="text-neutral-300">{group.hash.substring(0, 16)}...</span></span>
                              <span className="text-[11px] font-mono bg-neutral-900 px-2 py-0.5 rounded text-amber-400 border border-neutral-800">{group.fileSize}</span>
                            </div>
                            
                            {/* Side-by-side comparison grid split evenly */}
                            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                              {group.paths.map((file: any, fileIdx: number) => {
                                const isNas = file.path.includes("sftp://") || file.path.includes("/Volume/NAS");
                                return (
                                  <div key={fileIdx} className="bg-black border border-neutral-900 p-3 rounded flex flex-col justify-between">
                                    <div>
                                      <div className="flex items-center space-x-2 mb-1">
                                        <span className={`text-[9px] font-mono uppercase px-1.5 py-0.5 rounded ${isNas ? 'bg-blue-950 text-blue-400 border border-blue-900' : 'bg-emerald-950 text-emerald-400 border border-emerald-900'}`}>
                                          {isNas ? "🖥️ ASUSTOR NAS" : "💻 LOCAL MAC"}
                                        </span>
                                        <span className="text-[11px] font-mono text-neutral-400 truncate block max-w-[200px]">
                                          {file.name}
                                        </span>
                                      </div>
                                      <div className="text-[10px] font-mono text-neutral-600 break-all select-all p-1 bg-neutral-950 rounded border border-neutral-900 mt-1">
                                        {file.path}
                                      </div>
                                    </div>
                                    
                                    <button 
                                      onClick={() => handleDeleteClick(file.path)}
                                      className="mt-3 w-full bg-red-950/40 hover:bg-red-900/60 border border-red-900/50 hover:border-red-600 text-red-200 font-mono text-[10px] py-1 rounded transition-all tracking-wide uppercase cursor-pointer"
                                    >
                                      Vaporize Copy 🗑️
                                    </button>
                                  </div>
                                );
                              })}
                            </div>
                          </div>
                        ))}
                        {duplicateGroups.length === 0 && (
                          <div className="p-8 text-center text-neutral-600 font-mono">No duplicates detected in current database index.</div>
                        )}
                      </div>
                    ) : (
                      <div className="bg-neutral-900/20 border border-neutral-900 rounded-xl overflow-hidden">
                        <div className="max-h-[600px] overflow-y-auto font-mono text-xs">
                          <table className="w-full text-left border-collapse">
                            <thead>
                              <tr className="bg-neutral-900/60 border-b border-neutral-900 text-neutral-400 uppercase tracking-wider text-[10px] sticky top-0 z-10">
                                <th className="p-4">File Name</th>
                                <th className="p-4">Size</th>
                                <th className="p-4">Full BLAKE3 Hash</th>
                                <th className="p-4">Absolute Target Path</th>
                              </tr>
                            </thead>
                            <tbody className="divide-y divide-neutral-900">
                              {results.map((file, idx) => (
                                <tr 
                                  key={idx} 
                                  onClick={() => {
                                    setSelectedFile(file);
                                    setAiResult(null);
                                  }}
                                  className={`hover:bg-neutral-900/30 transition-colors cursor-pointer ${selectedFile?.file_path === file.file_path ? 'bg-blue-900/20 border-l-2 border-l-blue-500' : ''}`}
                                >
                                  <td className="p-4 font-medium text-neutral-200 max-w-[200px] truncate">{file.file_name}</td>
                                  <td className="p-4 text-neutral-400 whitespace-nowrap">{formatBytes(file.file_size)}</td>
                                  <td className="p-4 text-blue-400/80 font-semibold">{file.full_hash ? `${file.full_hash.substring(0, 12)}...` : "Skipped (Unique Size)"}</td>
                                  <td className="p-4 text-neutral-500 truncate max-w-[300px]" title={file.file_path as string}>{file.file_path}</td>
                                </tr>
                              ))}
                              {results.length === 0 && (
                                <tr>
                                  <td colSpan={4} className="p-8 text-center text-neutral-600 font-mono">Target directory contains 0 indexable system files.</td>
                                </tr>
                              )}
                            </tbody>
                          </table>
                        </div>
                      </div>
                    )}
                  </section>
                </div>

                {/* Right Side: The Sidebar AI Insights Panel (Takes up 30% width) */}
                {selectedFile && (
                  <div className="w-full lg:w-1/3 bg-neutral-900 border border-neutral-800 p-6 rounded-xl sticky top-6 animate-slide-in space-y-6 shadow-2xl shadow-black/50">
                    <div>
                      <h3 className="text-xs font-mono text-neutral-400 uppercase tracking-widest mb-4">// AI Insights Panel</h3>
                      <div className="space-y-2">
                        <p className="text-sm font-medium text-neutral-200 break-all">{selectedFile?.file_name}</p>
                        <p className="text-[10px] text-neutral-500 font-mono truncate" title={selectedFile?.file_path}>{selectedFile?.file_path}</p>
                      </div>
                    </div>

                    <button
                      onClick={handleAiAnalyze}
                      disabled={isAiLoading || !selectedFile}
                      className="w-full bg-blue-600 hover:bg-blue-500 disabled:bg-neutral-800 text-white font-mono text-xs py-3 rounded-lg font-medium transition-all duration-150 shadow-md shadow-blue-950/20 flex items-center justify-center gap-2 cursor-pointer disabled:cursor-not-allowed"
                    >
                      {isAiLoading ? (
                        <>
                          <div className="animate-spin rounded-full h-3 w-3 border-t border-b border-white"></div>
                          Gemma 4 Thinking...
                        </>
                      ) : (
                        "Analyze Content with Gemma 4"
                      )}
                    </button>

                    {aiResult && (
                      <div className="space-y-6 animate-fade-in border-t border-neutral-800 pt-6">
                        <div className="space-y-4">
                          <div className="flex justify-between items-start">
                            <label className="text-[10px] text-neutral-500 uppercase font-mono tracking-tighter">Confidence Score</label>
                            <span className={`px-2 py-0.5 rounded text-[10px] font-bold font-mono ${aiResult.confidence_score > 0.8 ? 'bg-emerald-500/20 text-emerald-400' : 'bg-yellow-500/20 text-yellow-400'}`}>
                              {(aiResult.confidence_score * 100).toFixed(1)}%
                            </span>
                          </div>
                          
                          <div className="space-y-1">
                            <label className="text-[10px] text-neutral-500 uppercase font-mono tracking-tighter">Suggested Subfolder</label>
                            <p className="text-sm font-mono text-blue-400 bg-blue-900/10 border border-blue-900/30 p-2 rounded">
                              {aiResult.suggested_subfolder}
                            </p>
                          </div>

                          <div className="space-y-1">
                            <label className="text-[10px] text-neutral-500 uppercase font-mono tracking-tighter">New Clean Name</label>
                            <p className="text-sm font-medium text-neutral-200">
                              {aiResult.new_clean_name}
                            </p>
                          </div>

                          <div className="space-y-1">
                            <label className="text-[10px] text-neutral-500 uppercase font-mono tracking-tighter">AI Reasoning</label>
                            <p className="text-[11px] text-neutral-400 leading-relaxed italic border-l-2 border-neutral-800 pl-3">
                              "{aiResult.reasoning}"
                            </p>
                          </div>
                        </div>
                      </div>
                    )}

                    {!aiResult && !isAiLoading && (
                      <div className="text-center py-8 border border-dashed border-neutral-800 rounded-lg">
                        <p className="text-[10px] text-neutral-600 font-mono">No active analysis for this node.</p>
                      </div>
                    )}

                    <button 
                      onClick={() => setSelectedFile(null)}
                      className="w-full text-[10px] font-mono text-neutral-500 hover:text-neutral-300 transition-colors"
                    >
                      [Close Panel]
                    </button>
                  </div>
                )}
              </div>
            )}
          </>
        ) : (
          <section className="bg-neutral-900/40 border border-neutral-900 p-8 rounded-xl space-y-8 animate-fade-in">
            <div className="space-y-2">
              <h2 className="text-sm font-mono text-neutral-400 uppercase tracking-wider">// Global Exclusion Engine</h2>
              <p className="text-xs text-neutral-500">Configure file extensions that the AI batching engine should skip during processing. (e.g., srt, mp4, log)</p>
            </div>

            <div className="space-y-4">
              <div className="flex gap-4">
                <input
                  type="text"
                  value={newExtension}
                  onChange={(e) => setNewExtension(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && newExtension.trim()) {
                      const ext = newExtension.trim().toLowerCase().replace(/^\./, '');
                      if (!excludedExtensions.includes(ext)) {
                        setExcludedExtensions([...excludedExtensions, ext]);
                      }
                      setNewExtension("");
                    }
                  }}
                  placeholder="Add extension (e.g. srt)"
                  className="flex-1 bg-neutral-950 border border-neutral-800 rounded-lg px-4 py-2.5 font-mono text-sm focus:outline-none focus:border-blue-500 text-neutral-200"
                />
                <button
                  onClick={() => {
                    if (newExtension.trim()) {
                      const ext = newExtension.trim().toLowerCase().replace(/^\./, '');
                      if (!excludedExtensions.includes(ext)) {
                        setExcludedExtensions([...excludedExtensions, ext]);
                      }
                      setNewExtension("");
                    }
                  }}
                  className="bg-blue-600 hover:bg-blue-500 text-white font-mono text-sm px-6 py-2.5 rounded-lg font-medium transition-colors cursor-pointer"
                >
                  Add
                </button>
              </div>

              <div className="flex flex-wrap gap-3 pt-4">
                {excludedExtensions.map((ext) => (
                  <div 
                    key={ext}
                    className="group flex items-center gap-2 bg-neutral-900 border border-neutral-800 px-3 py-1.5 rounded-md hover:border-red-900/50 transition-all"
                  >
                    <span className="text-xs font-mono text-neutral-300">.{ext}</span>
                    <button 
                      onClick={() => setExcludedExtensions(excludedExtensions.filter(e => e !== ext))}
                      className="text-neutral-600 hover:text-red-400 text-xs transition-colors"
                    >
                      ✕
                    </button>
                  </div>
                ))}
              </div>
            </div>

            <div className="pt-8 border-t border-neutral-900 flex justify-between items-center">
              <p className="text-[10px] font-mono text-neutral-600 italic">Settings are saved in local session memory.</p>
              <button 
                onClick={() => setExcludedExtensions(['srt', 'vtt', 'pyc', 'jsn', 'gz', 'html', 'png', 'jpg', 'gif'])}
                className="text-[10px] font-mono text-neutral-500 hover:text-neutral-300 underline underline-offset-4 decoration-neutral-800"
              >
                Reset to Defaults
              </button>
            </div>
          </section>
        )}
      </div>
    </main>
  );
}
