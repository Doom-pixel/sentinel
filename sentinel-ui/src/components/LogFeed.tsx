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
            default: return "";
        }
    };

    return (
        <div className="feed-container">
            <div className="feed-inner">
                {logs.length === 0 && <div className="log-empty">No output yet. Launch an agent to begin.</div>}

                {logs.map((log, i) => {
                    const isThought = log.message.startsWith("THOUGHT:");
                    const isWaiting = log.message.includes("Waiting for LLM response");
                    const displayMessage = isThought ? log.message.replace("THOUGHT:", "").trim() : log.message;

                    if (isThought) {
                        return (
                            <div key={i} className="thought-bubble">
                                <div className="thought-icon">{isWaiting ? '⏳' : '🧠'}</div>
                                <div className={`thought-text ${isWaiting ? 'pulsing' : ''}`}>
                                    {displayMessage}
                                </div>
                            </div>
                        );
                    }

                    return (
                        <div key={i} className={`log-entry ${levelClass(log.level)}`}>
                            <span className="log-level">{log.level.toUpperCase()}</span>
                            <span className="log-target">{log.target}</span>
                            <span className="log-message">{displayMessage}</span>
                        </div>
                    );
                })}
                <div ref={endRef} />
            </div>
        </div>
    );
}
