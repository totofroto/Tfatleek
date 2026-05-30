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
  identifiedMember: string | null;
}

interface FamilyMember {
  key: string;
  full_name: string;
  birth_date: string;
  role: string;
}

interface AppSettings {
  preset_paths: Record<string, string>;
  excluded_folders: string[];
  paperless_nas_ip: string;
  paperless_api_token: string;
}

export default function App() {
  const [activeTab, setActiveTab] = useState<string>('dashboard');
  const [shortcuts, setShortcuts] = useState<string[]>(['EXCLUSION SETTINGS']);
  const [ingestionContext, setIngestionContext] = useState<'PRIVATE' | 'OTHERS'>('PRIVATE');
  
  const [targetPath, setTargetPath] = useState("");
  const [status, setStatus] = useState<"idle" | "scanning" | "success" | "error" | "batching">("idle");
  const [results, setResults] = useState<ScanResult[]>([]);
  const [errorMessage, setErrorMessage] = useState("");
  
  const [selectedFile, setSelectedFile] = useState<ScanResult | null>(null);
  const [aiResult, setAiResult] = useState<AiClassificationResult | null>(null);
  const [isAiLoading, setIsAiLoading] = useState(false);
  const [batchProgress, setBatchProgress] = useState<ProgressPayload | null>(null);

  const [isDragging, setIsDragging] = useState(false);
  const [droppedFileLog, setDroppedFileLog] = useState<string | null>(null);

  const [pendingManifest, setPendingManifest] = useState<IngestionManifest | null>(null);
  const [selectedStorageTier, setSelectedStorageTier] = useState<"LOCAL" | "NAS" | "PAPERLESS">("LOCAL");

  const [familyRegistry, setFamilyRegistry] = useState<FamilyMember[]>([]);

  // Settings State
  const [appSettings, setAppSettings] = useState<AppSettings>({
    preset_paths: {},
    excluded_folders: [],
    paperless_nas_ip: "",
    paperless_api_token: ""
  });
  const [newExclusion, setNewExclusion] = useState("");

  const availableShortcuts = [
    { id: 'EXCLUSION SETTINGS', label: 'Exclusion Settings' },
    { id: 'FAMILY PRESETS', label: 'Family Presets' },
    { id: 'PATH MAPPINGS', label: 'Path Mappings' },
    { id: 'PAPERLESS CONFIG', label: 'Paperless Config' },
  ];

  useEffect(() => {
    const init = async () => {
      try {
        const [presets, settings] = await Promise.all([
          invoke<FamilyMember[]>("get_family_presets"),
          invoke<AppSettings>("get_settings")
        ]);
        setFamilyRegistry(presets);
        setAppSettings(settings);
      } catch (error) {
        console.error("Initialization Error:", error);
      }
    };
    init();
  }, []);

  useEffect(() => {
    const unlistenFileDrop = getCurrentWindow().onDragDropEvent((event) => {
      if (event.payload.type === 'enter') {
        setIsDragging(true);
      } else if (event.payload.type === 'leave') {
        setIsDragging(false);
      } else if (event.payload.type === 'drop') {
        setIsDragging(false);
        const droppedPaths = event.payload.paths;
        if (droppedPaths.length > 0) {
          handleDroppedIngestion(droppedPaths[0]);
        }
      }
    });

    return () => {
      unlistenFileDrop.then((unlisten) => unlisten());
    };
  }, [ingestionContext]);

  useEffect(() => {
    const unlistenPromise = listen<ProgressPayload>("scan-progress", (event) => {
      setBatchProgress(event.payload);
    });
    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  const handleDroppedIngestion = async (filePath: string) => {
    const fileName = filePath.split('/').pop() || filePath;
    setDroppedFileLog(`Analyzing [${ingestionContext}]: ${fileName}...`);
    
    try {
      const resultJson = await invoke<string>("process_single_dropped_file", { path: filePath, context: ingestionContext });
      const manifest: IngestionManifest = JSON.parse(resultJson);
      setPendingManifest(manifest);
      setDroppedFileLog(`✓ Analysis Complete: ${fileName}`);
    } catch (error) {
      setDroppedFileLog(`❌ Error: ${error}`);
    }
  };

  const handleExecuteFinalCommit = async () => {
    if (!pendingManifest) return;
    try {
      if (selectedStorageTier === "PAPERLESS") {
        setDroppedFileLog(`🚀 Transmitting to Paperless Vault...`);
        await invoke("submit_to_paperless_vault", {
          filePath: pendingManifest.originalPath,
          context: ingestionContext
        });
        setDroppedFileLog(`✓ Successfully vaulted in Paperless.`);
      } else {
        const result = await invoke<string>("execute_relocation_commit", {
          originalPath: pendingManifest.originalPath,
          suggestedName: pendingManifest.suggestedName,
          identifiedCategory: pendingManifest.identifiedCategory,
          detectedDate: pendingManifest.detectedDate,
          storageTier: selectedStorageTier,
          isTaxRelevant: pendingManifest.isTaxRelevant
        });
        setDroppedFileLog(`✓ ${result}`);
      }
      setPendingManifest(null);
      if (activeTab === 'dashboard' && status === 'success') handleScan();
    } catch (error) {
      setErrorMessage(`Commit Error: ${error}`);
      setPendingManifest(null);
    }
  };

  const handleScan = async () => {
    if (!targetPath.trim()) return;
    setStatus("scanning");
    setErrorMessage("");
    setSelectedFile(null);
    setAiResult(null);
    try {
      const rawJsonResponse = await invoke<string>("start_dedup_scan", { targetPath });
      setResults(JSON.parse(rawJsonResponse));
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
      setErrorMessage(`AI Error: ${error}`);
    } finally {
      setIsAiLoading(false);
    }
  };

  const handleBatchOrganize = async () => {
    if (!targetPath.trim()) return;
    setStatus("batching");
    try {
      await invoke("trigger_batch_ai_organization", { targetPath, customExcludes: [] });
      setStatus("success");
      handleScan();
    } catch (error) {
      setErrorMessage(`Batch Error: ${error}`);
      setStatus("error");
    }
  };

  const savePaperlessConfig = async () => {
    try {
      await invoke("update_paperless_settings", {
        nasIp: appSettings.paperless_nas_ip,
        api_token: appSettings.paperless_api_token
      });
      alert("Settings Secured.");
    } catch (error) {
      alert(`Save Failed: ${error}`);
    }
  };

  const addExclusion = async () => {
    if (!newExclusion) return;
    try {
      await invoke("add_excluded_folder", { folder: newExclusion });
      setAppSettings({ ...appSettings, excluded_folders: [...appSettings.excluded_folders, newExclusion] });
      setNewExclusion("");
    } catch (error) {
      alert(error);
    }
  };

  const removeExclusion = async (folder: string) => {
    try {
      await invoke("remove_excluded_folder", { folder });
      setAppSettings({ ...appSettings, excluded_folders: appSettings.excluded_folders.filter(f => f !== folder) });
    } catch (error) {
      alert(error);
    }
  };

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return "0 Bytes";
    const k = 1024;
    const sizes = ["Bytes", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  };

  const toggleShortcut = (id: string) => {
    if (shortcuts.includes(id)) {
      setShortcuts(shortcuts.filter(s => s !== id));
      if (activeTab === id) setActiveTab('settings');
    } else {
      setShortcuts([...shortcuts, id]);
    }
  };

  return (
    <main className="min-h-screen bg-neutral-950 text-neutral-100 p-8 font-sans selection:bg-blue-500/30">
      {/* Ingestion Manifest Modal */}
      {pendingManifest && (
        <div className="fixed inset-0 bg-black/90 backdrop-blur-sm flex items-center justify-center p-4 z-50">
          <div className="bg-neutral-900 border border-neutral-800 rounded-2xl p-8 max-w-2xl w-full shadow-[0_0_50px_-12px_rgba(0,0,0,0.5)]">
            <h2 className="text-lg font-bold text-blue-400 mb-6 flex items-center gap-2">
              <span className="animate-pulse">⚡</span> Manifest Verification
            </h2>
            <div className="space-y-6">
              <div className="grid grid-cols-2 gap-4 text-xs font-mono">
                <div className="bg-black/40 p-3 rounded-lg border border-neutral-800">
                  <span className="text-neutral-500 block mb-1 uppercase">Identified Category</span>
                  <span className="text-emerald-400 font-bold">{pendingManifest.identifiedCategory}</span>
                </div>
                <div className="bg-black/40 p-3 rounded-lg border border-neutral-800">
                  <span className="text-neutral-500 block mb-1 uppercase">Detected Date</span>
                  <span className="text-blue-400 font-bold">{pendingManifest.detectedDate}</span>
                </div>
              </div>
              <div>
                <label className="text-[10px] text-neutral-500 uppercase font-bold mb-1 block">Optimized Name</label>
                <input 
                  type="text" 
                  value={pendingManifest.suggestedName}
                  onChange={(e) => setPendingManifest({...pendingManifest, suggestedName: e.target.value})}
                  className="w-full bg-black border border-neutral-800 px-4 py-3 rounded-xl text-amber-400 focus:border-amber-500 outline-none font-mono text-sm"
                />
              </div>
              <div>
                <label className="text-[10px] text-neutral-500 uppercase font-bold mb-3 block">Storage Destination</label>
                <div className="grid grid-cols-3 gap-3">
                  {(["LOCAL", "NAS", "PAPERLESS"] as const).map(tier => (
                    <button 
                      key={tier}
                      onClick={() => setSelectedStorageTier(tier)}
                      className={`py-3 rounded-xl border text-[10px] font-black transition-all ${selectedStorageTier === tier ? 'bg-blue-600 border-blue-400 text-white' : 'bg-black border-neutral-800 text-neutral-500'}`}
                    >
                      {tier === "LOCAL" ? "💻 LOCAL" : tier === "NAS" ? "🖥️ NAS" : "🗄️ PAPERLESS"}
                    </button>
                  ))}
                </div>
              </div>
            </div>
            <div className="mt-8 flex justify-end gap-3">
              <button onClick={() => setPendingManifest(null)} className="px-6 py-2.5 text-neutral-500 text-xs font-bold uppercase">Discard</button>
              <button onClick={handleExecuteFinalCommit} className="px-8 py-2.5 bg-blue-600 hover:bg-blue-500 text-white text-xs font-black rounded-xl transition-all shadow-lg shadow-blue-900/20">Execute Relocation</button>
            </div>
          </div>
        </div>
      )}

      {/* Top Navigation */}
      <header className="max-w-6xl mx-auto mb-12">
        <div className="flex justify-between items-center mb-8">
          <h1 className="text-2xl font-black tracking-tighter uppercase italic">
            Tfatleek <span className="text-blue-600 text-sm not-italic ml-1 opacity-50">PRO</span>
          </h1>
          <div className="flex items-center gap-3">
             <div className="bg-neutral-900 px-4 py-1.5 rounded-full border border-neutral-800 text-[10px] font-mono text-neutral-400">
               OS: Darwin/M1 Pro
             </div>
             <button onClick={() => invoke("trigger_system_undo")} className="bg-emerald-600/10 text-emerald-500 border border-emerald-500/20 px-4 py-1.5 rounded-full text-[10px] font-bold hover:bg-emerald-600/20 transition-all">
               UNDO
             </button>
          </div>
        </div>
        <nav className="flex items-center gap-6 border-b border-neutral-900 pb-1">
          <button onClick={() => setActiveTab('dashboard')} className={`pb-3 text-xs font-black uppercase tracking-widest transition-all border-b-2 ${activeTab === 'dashboard' ? 'text-blue-500 border-blue-500' : 'text-neutral-600 border-transparent'}`}>Dashboard</button>
          <button onClick={() => setActiveTab('settings')} className={`pb-3 text-xs font-black uppercase tracking-widest transition-all border-b-2 ${activeTab === 'settings' ? 'text-blue-500 border-blue-500' : 'text-neutral-600 border-transparent'}`}>Settings</button>
          <div className="h-4 w-px bg-neutral-800 mx-2 mb-3"></div>
          {shortcuts.map(id => (
            <div key={id} className="flex items-center gap-1 group pb-3">
              <button onClick={() => setActiveTab(id)} className={`text-[10px] font-black uppercase tracking-widest transition-all border-b-2 ${activeTab === id ? 'text-emerald-500 border-emerald-500' : 'text-neutral-600 border-transparent hover:text-neutral-400'}`}>
                {availableShortcuts.find(s => s.id === id)?.label || id}
              </button>
              <button onClick={() => toggleShortcut(id)} className="text-[10px] text-neutral-800 hover:text-red-500 mb-1">✕</button>
            </div>
          ))}
          <button onClick={() => setActiveTab('settings')} className="pb-3 text-[10px] font-black text-neutral-700 hover:text-blue-500 mb-1 transition-colors">[+] PIN SHORTCUT</button>
        </nav>
      </header>

      <div className="max-w-6xl mx-auto">
        {activeTab === 'dashboard' && (
          <div className="grid grid-cols-12 gap-8">
            <div className="col-span-8 space-y-8">
              {/* Context Switcher */}
              <div className="flex bg-neutral-900 p-1.5 rounded-2xl border border-neutral-800 w-fit mx-auto">
                <button onClick={() => setIngestionContext('PRIVATE')} className={`px-8 py-2 rounded-xl text-[10px] font-black transition-all ${ingestionContext === 'PRIVATE' ? 'bg-blue-600 text-white shadow-xl shadow-blue-900/20' : 'text-neutral-500'}`}>🔒 PRIVATE MODE</button>
                <button onClick={() => setIngestionContext('OTHERS')} className={`px-8 py-2 rounded-xl text-[10px] font-black transition-all ${ingestionContext === 'OTHERS' ? 'bg-emerald-600 text-white shadow-xl shadow-emerald-900/20' : 'text-neutral-500'}`}>🏥 OTHERS MODE</button>
              </div>

              {/* Drop Zone */}
              <div className={`relative h-64 rounded-3xl border-2 border-dashed flex flex-col items-center justify-center transition-all ${isDragging ? 'border-blue-500 bg-blue-500/5 scale-[1.02]' : 'border-neutral-800 bg-neutral-900/30'}`}>
                <div className="text-4xl mb-4">{ingestionContext === 'PRIVATE' ? '📄' : '🩺'}</div>
                <h3 className="text-xs font-black uppercase tracking-widest text-neutral-400">Drop Ingestion Payload</h3>
                <p className="text-[10px] text-neutral-600 mt-2 font-mono">PDF • DOCX • DICOM • TXT</p>
                {droppedFileLog && <div className="absolute bottom-6 px-4 py-1.5 bg-black rounded-full border border-neutral-800 text-[10px] font-mono text-blue-400 animate-pulse">{droppedFileLog}</div>}
              </div>

              {/* Scan Control */}
              <div className="bg-neutral-900/50 p-6 rounded-3xl border border-neutral-800 flex gap-4">
                <input type="text" value={targetPath} onChange={(e) => setTargetPath(e.target.value)} placeholder="Target Directory Path..." className="flex-1 bg-black border border-neutral-800 rounded-xl px-4 py-3 text-sm font-mono focus:border-blue-500 outline-none"/>
                <button onClick={handleScan} className="bg-blue-600 px-8 py-3 rounded-xl text-xs font-black uppercase hover:bg-blue-500 transition-all">Scan</button>
                {results.length > 0 && <button onClick={handleBatchOrganize} className="bg-emerald-600 px-8 py-3 rounded-xl text-xs font-black uppercase hover:bg-emerald-500 transition-all">Batch</button>}
              </div>

              {/* Error Message */}
              {errorMessage && <div className="bg-red-900/20 border border-red-900/50 text-red-400 p-4 rounded-xl text-xs font-mono">{errorMessage}</div>}

              {/* Batch Progress */}
              {status === "batching" && batchProgress && (
                <div className="bg-neutral-900/50 p-6 rounded-3xl border border-neutral-800 space-y-4">
                  <div className="flex justify-between text-[10px] font-black uppercase">
                    <span className="text-blue-500">Processing: {batchProgress.current_file}</span>
                    <span>{batchProgress.percentage.toFixed(0)}%</span>
                  </div>
                  <div className="h-1.5 bg-black rounded-full overflow-hidden">
                    <div className="bg-blue-600 h-full transition-all" style={{width: `${batchProgress.percentage}%`}}></div>
                  </div>
                </div>
              )}

              {/* Scan Results Table */}
              {status === "success" && (
                <div className="bg-neutral-900/30 border border-neutral-800 rounded-3xl overflow-hidden">
                  <div className="max-h-96 overflow-y-auto">
                    <table className="w-full text-[10px] font-mono">
                      <thead className="bg-neutral-900 text-neutral-500 sticky top-0">
                        <tr>
                          <th className="p-4 text-left">Filename</th>
                          <th className="p-4 text-left">Size</th>
                          <th className="p-4 text-left">Hash</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-neutral-900">
                        {results.map((file, i) => (
                          <tr key={i} className={`hover:bg-neutral-800/50 cursor-pointer ${selectedFile?.file_path === file.file_path ? 'bg-blue-600/10' : ''}`} onClick={() => setSelectedFile(file)}>
                            <td className="p-4 text-neutral-300">{file.file_name}</td>
                            <td className="p-4 text-neutral-500">{formatBytes(file.file_size)}</td>
                            <td className="p-4 text-blue-900 font-bold">{file.full_hash?.slice(0, 12)}...</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              )}
            </div>

            {/* Side Panel: Identity + Insights */}
            <div className="col-span-4 space-y-6">
              {/* AI Insights Panel */}
              {selectedFile && (
                <div className="bg-blue-600/5 border border-blue-500/20 rounded-3xl p-6 space-y-4 animate-fade-in">
                  <h3 className="text-[10px] font-black uppercase tracking-widest text-blue-500">AI Intelligence</h3>
                  <div className="text-xs font-bold text-neutral-300 break-all">{selectedFile.file_name}</div>
                  <button onClick={handleAiAnalyze} disabled={isAiLoading} className="w-full bg-blue-600 py-2.5 rounded-xl text-[10px] font-black uppercase hover:bg-blue-500 transition-all disabled:opacity-50">{isAiLoading ? 'Analyzing...' : 'Deep Classification'}</button>
                  {aiResult && (
                    <div className="p-4 bg-black/40 rounded-2xl border border-neutral-800 space-y-3">
                       <div className="flex justify-between text-[8px] font-black uppercase">
                         <span className="text-neutral-500">Confidence</span>
                         <span className="text-emerald-400">{(aiResult.confidence_score * 100).toFixed(0)}%</span>
                       </div>
                       <div className="text-xs font-mono text-blue-400">{aiResult.suggested_subfolder}</div>
                       <div className="text-[10px] text-neutral-500 italic">"{aiResult.reasoning}"</div>
                    </div>
                  )}
                </div>
              )}

              <div className="bg-neutral-900/50 border border-neutral-800 rounded-3xl p-6">
                <h3 className="text-[10px] font-black uppercase tracking-widest text-blue-500 mb-6">Family Registry</h3>
                <div className="space-y-4">
                  {familyRegistry.map(member => (
                    <div key={member.key} className="bg-black/50 p-4 rounded-2xl border border-neutral-800">
                      <div className="flex justify-between text-[8px] font-black uppercase text-neutral-600 mb-2">
                        <span>{member.role}</span>
                        <span>{member.key}</span>
                      </div>
                      <div className="text-xs font-bold text-neutral-300">{member.full_name}</div>
                      <div className="text-[10px] text-neutral-500 mt-1">{member.birth_date}</div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        )}

        {activeTab === 'settings' && (
          <div className="grid grid-cols-2 gap-8 animate-fade-in">
            <div className="bg-neutral-900/50 border border-neutral-800 rounded-3xl p-8 space-y-6">
              <h2 className="text-xs font-black uppercase tracking-widest text-blue-500">Paperless Config</h2>
              <div className="space-y-4">
                <div>
                  <label className="text-[10px] text-neutral-500 uppercase font-black block mb-2">NAS IP Address</label>
                  <input type="text" value={appSettings.paperless_nas_ip} onChange={(e) => setAppSettings({...appSettings, paperless_nas_ip: e.target.value})} placeholder="192.168.1.100" className="w-full bg-black border border-neutral-800 rounded-xl px-4 py-3 text-sm font-mono focus:border-blue-500 outline-none"/>
                </div>
                <div>
                  <label className="text-[10px] text-neutral-500 uppercase font-black block mb-2">REST API Token</label>
                  <input type="password" value={appSettings.paperless_api_token} onChange={(e) => setAppSettings({...appSettings, paperless_api_token: e.target.value})} placeholder="••••••••••••••••" className="w-full bg-black border border-neutral-800 rounded-xl px-4 py-3 text-sm font-mono focus:border-blue-500 outline-none text-emerald-500"/>
                </div>
                <button onClick={savePaperlessConfig} className="w-full bg-blue-600 py-3 rounded-xl text-[10px] font-black uppercase hover:bg-blue-500 transition-all">Secure Credentials</button>
              </div>
            </div>
            <div className="bg-neutral-900/50 border border-neutral-800 rounded-3xl p-8 space-y-6">
              <h2 className="text-xs font-black uppercase tracking-widest text-red-500">Exclusion Manager</h2>
              <div className="flex gap-2">
                <input type="text" value={newExclusion} onChange={(e) => setNewExclusion(e.target.value)} placeholder="Folder name or path..." className="flex-1 bg-black border border-neutral-800 rounded-xl px-4 py-2 text-xs font-mono focus:border-red-500 outline-none"/>
                <button onClick={addExclusion} className="bg-neutral-800 px-6 py-2 rounded-xl text-[10px] font-black uppercase">+</button>
              </div>
              <div className="flex flex-wrap gap-2">
                {appSettings.excluded_folders.map(folder => (
                  <div key={folder} className="bg-black border border-neutral-800 px-3 py-1 rounded-full text-[10px] font-mono flex items-center gap-2">
                    {folder}
                    <button onClick={() => removeExclusion(folder)} className="text-neutral-600 hover:text-red-500">✕</button>
                  </div>
                ))}
              </div>
            </div>

            <div className="col-span-2 bg-neutral-900/50 border border-neutral-800 rounded-3xl p-8 space-y-6">
              <h2 className="text-xs font-black uppercase tracking-widest text-emerald-500">Path Mappings</h2>
              <div className="grid grid-cols-2 gap-6">
                 <div className="space-y-4">
                    <div className="flex gap-2">
                      <input id="new-path-name" type="text" placeholder="Preset Name (e.g. NAS_DOCS)" className="flex-1 bg-black border border-neutral-800 rounded-xl px-4 py-2 text-xs font-mono outline-none"/>
                      <input id="new-path-value" type="text" placeholder="/Volumes/NAS/Docs" className="flex-1 bg-black border border-neutral-800 rounded-xl px-4 py-2 text-xs font-mono outline-none"/>
                      <button onClick={async () => {
                        const name = (document.getElementById('new-path-name') as HTMLInputElement).value;
                        const path = (document.getElementById('new-path-value') as HTMLInputElement).value;
                        if (name && path) {
                          await invoke('add_preset_path', { name, path });
                          const settings = await invoke<AppSettings>("get_settings");
                          setAppSettings(settings);
                        }
                      }} className="bg-emerald-600 px-4 py-2 rounded-xl text-[10px] font-black uppercase">Add</button>
                    </div>
                 </div>
                 <div className="space-y-2">
                    {Object.entries(appSettings.preset_paths).map(([name, path]) => (
                      <div key={name} className="flex justify-between items-center bg-black/40 border border-neutral-800 p-3 rounded-xl">
                        <span className="text-[10px] font-black text-neutral-400 uppercase">{name}</span>
                        <span className="text-[10px] font-mono text-emerald-500 truncate max-w-[200px]">{path}</span>
                      </div>
                    ))}
                 </div>
              </div>
            </div>

            <div className="col-span-2 bg-neutral-900/50 border border-neutral-800 rounded-3xl p-8 space-y-6">
              <h2 className="text-xs font-black uppercase tracking-widest text-blue-500">Shortcut Tab Manager</h2>
              <div className="grid grid-cols-4 gap-4">
                {availableShortcuts.map(s => (
                  <button key={s.id} onClick={() => toggleShortcut(s.id)} className={`p-6 rounded-3xl border text-center transition-all ${shortcuts.includes(s.id) ? 'bg-emerald-600/10 border-emerald-500/50 text-emerald-500' : 'bg-black border-neutral-800 text-neutral-600'}`}>
                    <div className="text-2xl mb-2">{shortcuts.includes(s.id) ? '📌' : '📍'}</div>
                    <div className="text-[10px] font-black uppercase tracking-widest">{s.label}</div>
                  </button>
                ))}
              </div>
            </div>
          </div>
        )}

        {/* Dynamic Shortcuts */}
        {activeTab === 'EXCLUSION SETTINGS' && (
          <div className="bg-neutral-900/50 border border-neutral-800 rounded-3xl p-8 animate-fade-in">
             <h2 className="text-xs font-black uppercase tracking-widest text-red-500 mb-6">Quick Exclusion Access</h2>
             <div className="flex gap-2 mb-4">
                <input type="text" value={newExclusion} onChange={(e) => setNewExclusion(e.target.value)} className="flex-1 bg-black border border-neutral-800 rounded-xl px-4 py-3 text-sm font-mono focus:border-red-500 outline-none"/>
                <button onClick={addExclusion} className="bg-blue-600 px-8 rounded-xl text-xs font-black uppercase">Add</button>
             </div>
             <div className="flex flex-wrap gap-2">
                {appSettings.excluded_folders.map(folder => (
                  <div key={folder} className="bg-black border border-neutral-800 px-4 py-2 rounded-xl text-xs font-mono flex items-center gap-2">
                    {folder}
                    <button onClick={() => removeExclusion(folder)} className="text-red-500">✕</button>
                  </div>
                ))}
             </div>
          </div>
        )}

        {activeTab === 'FAMILY PRESETS' && (
          <div className="bg-neutral-900/50 border border-neutral-800 rounded-3xl p-8 animate-fade-in">
             <h2 className="text-xs font-black uppercase tracking-widest text-blue-500 mb-6">Quick Family Presets</h2>
             <div className="grid grid-cols-2 gap-4">
                {familyRegistry.map(member => (
                  <div key={member.key} className="bg-black p-4 rounded-2xl border border-neutral-800">
                    <div className="text-[8px] font-black text-neutral-500 uppercase mb-2">{member.role}</div>
                    <div className="text-sm font-bold text-neutral-300 mb-1">{member.full_name}</div>
                    <div className="text-[10px] text-neutral-500">{member.birth_date}</div>
                  </div>
                ))}
             </div>
          </div>
        )}

        {activeTab === 'PATH MAPPINGS' && (
          <div className="bg-neutral-900/50 border border-neutral-800 rounded-3xl p-8 animate-fade-in">
             <h2 className="text-xs font-black uppercase tracking-widest text-emerald-500 mb-6">Quick Path Mappings</h2>
             <div className="space-y-4">
                {Object.entries(appSettings.preset_paths).map(([name, path]) => (
                  <div key={name} className="flex flex-col gap-1">
                    <span className="text-[10px] font-black text-neutral-600 uppercase">{name}</span>
                    <div className="w-full bg-black border border-neutral-800 rounded-xl px-4 py-3 text-xs font-mono text-emerald-500">{path}</div>
                  </div>
                ))}
             </div>
          </div>
        )}

        {activeTab === 'PAPERLESS CONFIG' && (
          <div className="bg-neutral-900/50 border border-neutral-800 rounded-3xl p-8 animate-fade-in">
             <h2 className="text-xs font-black uppercase tracking-widest text-blue-500 mb-6">Quick Paperless Access</h2>
             <div className="grid grid-cols-2 gap-4 mb-6">
                <input type="text" value={appSettings.paperless_nas_ip} onChange={(e) => setAppSettings({...appSettings, paperless_nas_ip: e.target.value})} className="bg-black border border-neutral-800 rounded-xl px-4 py-3 text-sm font-mono" placeholder="IP Address"/>
                <input type="password" value={appSettings.paperless_api_token} onChange={(e) => setAppSettings({...appSettings, paperless_api_token: e.target.value})} className="bg-black border border-neutral-800 rounded-xl px-4 py-3 text-sm font-mono" placeholder="API Token"/>
             </div>
             <button onClick={savePaperlessConfig} className="w-full bg-blue-600 py-3 rounded-xl text-xs font-black uppercase">Secure New Config</button>
          </div>
        )}
      </div>
    </main>
  );
}
