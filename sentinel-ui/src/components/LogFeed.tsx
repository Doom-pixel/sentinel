import { useEffect, useRef } from "react";

interface LogEntry { level: string; target: string; message: string; }

export default function LogFeed({ logs }: { logs: LogEntry[] }) {
  const endRef = useRef<HTMLDivElement>(null);
  useEffect(() => { endRef.current?.scrollIntoView({ behavior: "smooth" }); }, [logs]);

  const levelClass = (level: string) => {
    switch (level.toLowerCase()) {
      case "error": return "log-error";
      case "warn": return "log-warn";
      case "info": return "log-info";
      case "debug": return "log-debug";
      default: return "log-trace";
    }
  };

  return (
    <div className="card log-feed">
      <h2>Live Log Feed</h2>
      <div className="log-container">
        {logs.length === 0 && <div className="log-empty">No logs yet. Launch an agent to begin.</div>}
        {logs.map((log, i) => (
          <div key={i} className={`log-entry ${levelClass(log.level)}`}>
            <span className="log-level">{log.level.toUpperCase()}</span>
            <span className="log-target">{log.target}</span>
            <span className="log-message">{log.message}</span>
          </div>
        ))}
        <div ref={endRef} />
      </div>
    </div>
  );
}
