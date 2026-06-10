import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface WatcherHealth {
  status: string;
  timestamp: string;
  seconds_ago: number;
  pid: number | null;
  file_exists: boolean;
  interval_seconds: number | null;
}

interface ServiceStatus {
  name: string;
  url: string;
  status: "checking" | "online" | "offline";
}

interface Props {
  heartbeatPath: string;
}

function logLineColor(line: string): string {
  const u = line.toUpperCase();
  if (u.includes("ERROR")) return "text-red-400";
  if (u.includes("WARN")) return "text-yellow-400";
  if (u.includes("DEDUP")) return "text-blue-400";
  if (u.includes("MANIFEST")) return "text-purple-400";
  if (u.includes("RULES")) return "text-teal-400";
  if (u.includes("SAFE_LANDING") || u.includes("RETRY")) return "text-orange-400";
  return "text-neutral-300";
}

export default function WatcherHealthDashboard({ heartbeatPath }: Props) {
  const [health, setHealth] = useState<WatcherHealth | null>(null);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [secondsSinceUpdate, setSecondsSinceUpdate] = useState(0);
  const [services, setServices] = useState<ServiceStatus[]>([
    { name: "Paperless-ngx", url: "http://192.168.254.15:25680", status: "checking" },
    { name: "Ollama", url: "http://192.168.254.15:11434", status: "checking" },
  ]);

  const logEndRef = useRef<HTMLDivElement>(null);
  const logPath = heartbeatPath.replace("watcher_heartbeat.json", "watcher.log");

  const fetchData = useCallback(async () => {
    try {
      const [h, lines] = await Promise.all([
        invoke<WatcherHealth>("get_watcher_health", { heartbeatPath }),
        invoke<string[]>("get_watcher_log_tail", { logPath, lines: 30 }),
      ]);
      setHealth(h);
      setLogLines(lines);
      setSecondsSinceUpdate(0);
    } catch (e) {
      console.error("Failed to fetch watcher data:", e);
    }
  }, [heartbeatPath, logPath]);

  const checkServices = useCallback(async () => {
    const probe = async (name: string, url: string) => {
      try {
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), 4000);
        await fetch(url, { signal: controller.signal, mode: "no-cors" });
        clearTimeout(timer);
        setServices((prev) =>
          prev.map((s) => (s.name === name ? { ...s, status: "online" } : s))
        );
      } catch {
        setServices((prev) =>
          prev.map((s) => (s.name === name ? { ...s, status: "offline" } : s))
        );
      }
    };
    await Promise.allSettled([
      probe("Paperless-ngx", "http://192.168.254.15:25680/"),
      probe("Ollama", "http://192.168.254.15:11434/api/tags"),
    ]);
  }, []);

  // Initial load + 30s refresh
  useEffect(() => {
    fetchData();
    checkServices();
    const dataTimer = setInterval(fetchData, 30000);
    const svcTimer = setInterval(checkServices, 60000);
    return () => {
      clearInterval(dataTimer);
      clearInterval(svcTimer);
    };
  }, [fetchData, checkServices]);

  // Seconds-since-update ticker
  useEffect(() => {
    const t = setInterval(() => setSecondsSinceUpdate((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, []);

  // Auto-scroll log to bottom when lines change
  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logLines]);

  const nasStatus =
    health === null
      ? "checking"
      : health.file_exists && health.status === "alive"
      ? "online"
      : health.file_exists && health.status === "stale"
      ? "stale"
      : "offline";

  const headerStyle =
    health === null
      ? { bg: "bg-neutral-900/50 border-neutral-800", dot: "bg-neutral-600", label: "CHECKING...", labelCls: "text-neutral-400" }
      : health.status === "alive"
      ? { bg: "bg-emerald-900/10 border-emerald-800/30", dot: "bg-emerald-500", label: "WATCHER ONLINE", labelCls: "text-emerald-400" }
      : health.status === "stale"
      ? { bg: "bg-yellow-900/10 border-yellow-800/30", dot: "bg-yellow-500", label: "WATCHER STALE", labelCls: "text-yellow-400" }
      : { bg: "bg-red-900/10 border-red-800/30", dot: "bg-red-500", label: "WATCHER OFFLINE", labelCls: "text-red-400" };

  return (
    <div className="space-y-4 flex flex-col" style={{ height: "calc(100vh - 200px)" }}>
      {/* ── Status header ── */}
      <div className={`p-5 rounded-2xl border ${headerStyle.bg} flex-shrink-0`}>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className={`w-3 h-3 rounded-full ${headerStyle.dot} ${health?.status === "alive" ? "animate-pulse" : ""}`} />
            <span className={`text-sm font-black uppercase tracking-widest ${headerStyle.labelCls}`}>
              {headerStyle.label}
            </span>
          </div>
          <div className="flex items-center gap-3">
            <span className="text-[10px] font-mono text-neutral-600">
              Updated {secondsSinceUpdate}s ago
            </span>
            <button
              onClick={() => { fetchData(); checkServices(); }}
              className="bg-neutral-800 hover:bg-neutral-700 border border-neutral-700 px-3 py-1.5 rounded-xl text-[10px] font-black uppercase tracking-widest transition-all text-neutral-300"
            >
              ↻ Refresh
            </button>
          </div>
        </div>

        {health?.file_exists && (
          <div className="mt-3 flex flex-wrap gap-5 text-[10px] font-mono">
            <span className="text-neutral-500">
              Last heartbeat:{" "}
              <span className="text-neutral-200">
                {health.seconds_ago >= 0 ? `${health.seconds_ago}s ago` : "unknown"}
              </span>
            </span>
            {health.pid !== null && (
              <span className="text-neutral-500">
                PID: <span className="text-neutral-200">{health.pid}</span>
              </span>
            )}
            {health.interval_seconds !== null && (
              <span className="text-neutral-500">
                Interval: <span className="text-neutral-200">{health.interval_seconds}s</span>
              </span>
            )}
            <span className="text-neutral-700 truncate max-w-xs" title={health.timestamp}>
              {health.timestamp}
            </span>
          </div>
        )}

        {!health?.file_exists && health !== null && (
          <p className="mt-2 text-[10px] font-mono text-neutral-600">
            Heartbeat file not found at {heartbeatPath}
          </p>
        )}
      </div>

      {/* ── Pipeline service status ── */}
      <div className="bg-neutral-900/50 border border-neutral-800 rounded-2xl p-5 flex-shrink-0">
        <h3 className="text-[10px] font-black uppercase tracking-widest text-neutral-500 mb-4">
          Pipeline Status
        </h3>
        <div className="space-y-3">
          {services.map((svc) => (
            <div key={svc.name} className="flex items-center justify-between">
              <div className="flex items-center gap-2.5">
                <div
                  className={`w-2 h-2 rounded-full flex-shrink-0 ${
                    svc.status === "online"
                      ? "bg-emerald-500"
                      : svc.status === "offline"
                      ? "bg-red-500"
                      : "bg-neutral-600 animate-pulse"
                  }`}
                />
                <span className="text-xs font-bold text-neutral-300">{svc.name}</span>
              </div>
              <div className="flex items-center gap-3">
                <span className="text-[10px] font-mono text-neutral-700">{svc.url}</span>
                <StatusBadge status={svc.status === "checking" ? "checking" : svc.status} />
              </div>
            </div>
          ))}

          {/* NAS Inbox — derived from heartbeat */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2.5">
              <div
                className={`w-2 h-2 rounded-full flex-shrink-0 ${
                  nasStatus === "online"
                    ? "bg-emerald-500"
                    : nasStatus === "stale"
                    ? "bg-yellow-500"
                    : nasStatus === "checking"
                    ? "bg-neutral-600 animate-pulse"
                    : "bg-red-500"
                }`}
              />
              <span className="text-xs font-bold text-neutral-300">NAS Inbox</span>
            </div>
            <div className="flex items-center gap-3">
              <span className="text-[10px] font-mono text-neutral-700">
                /Volumes/Papers/Tfatleek_Inbox
              </span>
              <StatusBadge
                status={
                  nasStatus === "online"
                    ? "online"
                    : nasStatus === "stale"
                    ? "stale"
                    : nasStatus === "checking"
                    ? "checking"
                    : "offline"
                }
                label={nasStatus === "online" ? "reachable" : nasStatus === "stale" ? "stale" : undefined}
              />
            </div>
          </div>
        </div>
      </div>

      {/* ── Log tail ── */}
      <div className="flex-1 bg-neutral-950 border border-neutral-800 rounded-2xl flex flex-col overflow-hidden min-h-0">
        <div className="flex items-center justify-between px-4 py-3 border-b border-neutral-900 flex-shrink-0">
          <span className="text-[10px] font-black uppercase tracking-widest text-neutral-500">
            Recent Activity
          </span>
          <span className="text-[10px] font-mono text-neutral-700">
            last {logLines.length} lines
          </span>
        </div>

        {logLines.length === 0 ? (
          <div className="flex-1 flex items-center justify-center text-neutral-700 text-xs font-mono">
            {health === null ? "Loading..." : "No log data — watcher.log not found"}
          </div>
        ) : (
          <div className="flex-1 overflow-y-auto p-4">
            {logLines.map((line, i) => (
              <div
                key={i}
                className={`text-[10px] font-mono leading-relaxed whitespace-pre-wrap break-all ${logLineColor(line)}`}
              >
                {line}
              </div>
            ))}
            <div ref={logEndRef} />
          </div>
        )}
      </div>
    </div>
  );
}

function StatusBadge({
  status,
  label,
}: {
  status: "online" | "offline" | "stale" | "checking";
  label?: string;
}) {
  const text = label ?? status;
  const cls =
    status === "online"
      ? "text-emerald-400 bg-emerald-900/20 border-emerald-800/30"
      : status === "stale"
      ? "text-yellow-400 bg-yellow-900/20 border-yellow-800/30"
      : status === "offline"
      ? "text-red-400 bg-red-900/20 border-red-800/30"
      : "text-neutral-500 bg-neutral-800 border-neutral-700";
  return (
    <span className={`text-[9px] font-black uppercase px-2 py-0.5 rounded border ${cls}`}>
      {status === "checking" ? "..." : text}
    </span>
  );
}
