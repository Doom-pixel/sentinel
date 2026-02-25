import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import LaunchPanel from "./components/LaunchPanel";
import LogFeed from "./components/LogFeed";
import HitlModal from "./components/HitlModal";

interface LogEntry { level: string; target: string; message: string; }
interface ManifestInfo { id: string; action_description: string; parameters_json: string; risk_level: string; }

function App() {
    const [logs, setLogs] = useState<LogEntry[]>([]);
    const [isRunning, setIsRunning] = useState(false);
    const [hitlRequest, setHitlRequest] = useState<ManifestInfo | null>(null);

    useEffect(() => {
        const unlistenLog = listen<LogEntry>("sentinel://log", (event) => {
            setLogs((prev) => [...prev.slice(-500), event.payload]);
        });
        const unlistenHitl = listen<ManifestInfo>("sentinel://hitl-request", (event) => {
            setHitlRequest(event.payload);
        });
        const unlistenStop = listen("sentinel://agent-stopped", () => { setIsRunning(false); });
        return () => { unlistenLog.then((f) => f()); unlistenHitl.then((f) => f()); unlistenStop.then((f) => f()); };
    }, []);

    const handleApprove = useCallback(async (manifestId: string, approved: boolean) => {
        await invoke("handle_hitl_approval", { manifestId, approved });
        setHitlRequest(null);
    }, []);

    return (
        <div className="app">
            <header className="header">
                <div className="header-brand">
                    <h1>Sentinel</h1>
                    <span className="header-tag">v0.1.0</span>
                </div>
                <div className="header-status">
                    <span className={`status-dot ${isRunning ? "active" : ""}`} />
                    <span>{isRunning ? "Running" : "Idle"}</span>
                </div>
            </header>

            <main className="main-container">
                {/* 
                  If not running and no logs, show the clean launch pad.
                  Otherwise, transition the launch pad upwards or aside and show the log feed.
                */}
                <div className={`launch-wrapper ${isRunning || logs.length > 0 ? "minimized" : "centered"}`}>
                    <LaunchPanel isRunning={isRunning} setIsRunning={setIsRunning} setLogs={setLogs} />
                </div>

                {(isRunning || logs.length > 0) && (
                    <div className="feed-wrapper">
                        <LogFeed logs={logs} />
                    </div>
                )}
            </main>

            {hitlRequest && <HitlModal manifest={hitlRequest} onDecision={handleApprove} />}
        </div>
    );
}

export default App;
