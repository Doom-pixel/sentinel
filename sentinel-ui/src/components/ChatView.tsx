import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import Markdown from "./Markdown";

interface LogEntry { level: string; target: string; message: string; }

interface Props {
    agentId: string;
    logs: LogEntry[];
    status: "running" | "completed" | "error";
    agentLabel?: string;
    onClose: () => void;
    onSendMessage: (message: string) => void;
}

interface ChatItem {
    type: "thought" | "log-group" | "phase" | "report" | "finding" | "gui-start" | "gui-stop" | "user-message" | "sub-agent";
    content: string;
    logs?: LogEntry[];
    level?: string;
}

function parseLogs(logs: LogEntry[]): ChatItem[] {
    const items: ChatItem[] = [];
    let currentLogGroup: LogEntry[] = [];
    let pendingThought = "";

    const flushLogGroup = () => {
        if (currentLogGroup.length > 0) {
            items.push({ type: "log-group", content: `${currentLogGroup.length} log entries`, logs: [...currentLogGroup] });
            currentLogGroup = [];
        }
    };

    const flushThought = () => {
        if (pendingThought) {
            items.push({ type: "thought", content: pendingThought.trim() });
            pendingThought = "";
        }
    };

    for (let i = 0; i < logs.length; i++) {
        const log = logs[i];
        const msg = log.message;

        // User messages
        if (msg.startsWith("USER:")) {
            flushThought();
            flushLogGroup();
            items.push({ type: "user-message", content: msg.replace("USER:", "").trim() });
            continue;
        }

        // THOUGHT messages — merge consecutive ones
        if (msg.startsWith("THOUGHT:") || msg.startsWith("agent THOUGHT:")) {
            flushLogGroup();
            const content = msg.replace("agent THOUGHT:", "").replace("THOUGHT:", "").trim();

            // Skip "Waiting for LLM" messages — those go to logs
            if (content.startsWith("Waiting for LLM")) {
                currentLogGroup.push(log);
                continue;
            }

            if (pendingThought) {
                // If this is a new thought (starts with a tool or action), flush the old one
                if (content.startsWith("Using tool:") || content.startsWith("Task received:") ||
                    content.startsWith("✅") || content.startsWith("❌") || content.startsWith("⚠️") ||
                    content.startsWith("🔀") || content.startsWith("This task requires")) {
                    flushThought();
                    pendingThought = content;
                } else {
                    // Continue the previous thought (multi-line LLM response)
                    pendingThought += "\n" + content;
                }
            } else {
                pendingThought = content;
            }
            continue;
        }

        // Lines without any prefix that follow a THOUGHT — merge them
        if (pendingThought && !msg.startsWith("[") && !msg.includes("GUI_ACTIVE") &&
            !msg.includes("container stopped") && !msg.includes("Tool result") &&
            !msg.startsWith("172.17.") && !msg.includes("websockify") && !msg.includes("connecting to:") &&
            !msg.includes("code 404") && !msg.includes("Plain non-SSL")) {
            pendingThought += "\n" + msg;
            continue;
        }

        // Flush any pending thought before processing other message types
        flushThought();

        if (msg.includes("GUI_ACTIVE:true") || msg.includes("Opening the live view")) {
            flushLogGroup();
            items.push({ type: "gui-start", content: "Browser view activated" });
            continue;
        }

        if (msg.includes("GUI_ACTIVE:false")) {
            flushLogGroup();
            items.push({ type: "gui-stop", content: "Browser view closed" });
            continue;
        }

        if (msg.startsWith("[Phase") || msg.startsWith("──") || msg.startsWith("═══")) {
            flushLogGroup();
            items.push({ type: "phase", content: msg });
            continue;
        }

        if (msg.includes("Sub-agent") || msg.includes("sub-task") || msg.includes("🔀")) {
            flushLogGroup();
            items.push({ type: "sub-agent", content: msg });
            continue;
        }

        if (msg.includes("Report written") || msg.includes("Task complete") || msg.includes("boot sequence complete")) {
            flushLogGroup();
            items.push({ type: "report", content: msg });
            continue;
        }

        currentLogGroup.push(log);
    }

    flushThought();
    flushLogGroup();
    return items;
}

export default function ChatView({ agentId, logs, status, agentLabel, onClose, onSendMessage }: Props) {
    const endRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLTextAreaElement>(null);
    const [expandedGroups, setExpandedGroups] = useState<Set<number>>(new Set());
    const [novncPort, setNovncPort] = useState<number | null>(null);
    const [showLiveView, setShowLiveView] = useState(false);
    const [isLiveMode, setIsLiveMode] = useState(true);
    const [inputValue, setInputValue] = useState("");

    useEffect(() => {
        endRef.current?.scrollIntoView({ behavior: "smooth" });
    }, [logs]);

    useEffect(() => {
        invoke<number>("get_novnc_port", { agentId })
            .then(port => setNovncPort(port))
            .catch(() => setNovncPort(null));
    }, [agentId]);

    useEffect(() => {
        const hasGuiStart = logs.some(l => l.message.includes("Opening the live view") || l.message.includes("GUI_ACTIVE:true"));
        const hasGuiStop = logs.some(l => l.message.includes("GUI_ACTIVE:false"));
        if (hasGuiStart && !hasGuiStop) {
            setShowLiveView(true);
            setIsLiveMode(true);
        }
    }, [logs]);

    const toggleGroup = (index: number) => {
        setExpandedGroups(prev => {
            const next = new Set(prev);
            if (next.has(index)) next.delete(index); else next.add(index);
            return next;
        });
    };

    const handleSend = () => {
        const msg = inputValue.trim();
        if (!msg) return;
        onSendMessage(msg);
        setInputValue("");
        inputRef.current?.focus();
    };

    const chatItems = parseLogs(logs);

    return (
        <div className="chat-wrapper">
            <div className="chat-header">
                <div className="chat-header-left">
                    <div className={`chat-status-dot ${status}`} />
                    <div>
                        <h2>{agentLabel || `Agent ${agentId.slice(0, 8)}`}</h2>
                        <span className="chat-status-text">
                            {status === "running" ? "Working..." : status === "completed" ? "Completed" : "Error"}
                        </span>
                    </div>
                </div>
                <div className="chat-header-right">
                    {novncPort && (
                        <button
                            className={`btn-live-view ${showLiveView ? 'active' : ''}`}
                            onClick={() => setShowLiveView(!showLiveView)}
                            title="Toggle Live View"
                        >
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                                <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
                                <line x1="8" y1="21" x2="16" y2="21" />
                                <line x1="12" y1="17" x2="12" y2="21" />
                            </svg>
                            <span>{showLiveView ? "Hide" : "Live"}</span>
                        </button>
                    )}
                    <button className="btn-close-view" onClick={onClose}>
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                            <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
                        </svg>
                    </button>
                </div>
            </div>

            {showLiveView && novncPort && (
                <div className="live-view-container">
                    <div className="live-view-header">
                        <div className="live-view-title">
                            <span className={`live-dot ${isLiveMode ? 'live' : ''}`} />
                            <span>{isLiveMode ? "Live" : "Replay"}</span>
                        </div>
                        <div className="live-view-controls">
                            {status === "completed" && (
                                <button className={`btn-replay-toggle ${!isLiveMode ? 'active' : ''}`} onClick={() => setIsLiveMode(!isLiveMode)}>
                                    {isLiveMode ? "Replay" : "Live"}
                                </button>
                            )}
                            <button className="btn-close-live" onClick={() => setShowLiveView(false)}>
                                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                                    <polyline points="4 14 10 14 10 20" /><polyline points="20 10 14 10 14 4" />
                                    <line x1="14" y1="10" x2="21" y2="3" /><line x1="3" y1="21" x2="10" y2="14" />
                                </svg>
                            </button>
                        </div>
                    </div>
                    {isLiveMode ? (
                        <iframe className="live-view-frame" src={`http://localhost:${novncPort}/vnc.html?autoconnect=true&resize=scale&quality=6`} title="Agent Live View" />
                    ) : (
                        <div className="replay-container">
                            <p className="replay-placeholder">Replay will be available when screen recording is complete.</p>
                        </div>
                    )}
                </div>
            )}

            <div className="chat-container">
                <div className="chat-inner">
                    {chatItems.length === 0 && <div className="chat-empty">Waiting for agent output...</div>}

                    {chatItems.map((item, i) => {
                        if (item.type === "user-message") {
                            return (
                                <div key={i} className="chat-bubble user">
                                    <div className="chat-bubble-content user-bubble">
                                        <Markdown content={item.content} />
                                    </div>
                                    <div className="chat-bubble-avatar user-avatar">U</div>
                                </div>
                            );
                        }

                        if (item.type === "thought") {
                            return (
                                <div key={i} className="chat-bubble agent">
                                    <div className="chat-bubble-avatar">S</div>
                                    <div className="chat-bubble-content"><Markdown content={item.content} /></div>
                                </div>
                            );
                        }

                        if (item.type === "gui-start" || item.type === "gui-stop") {
                            return (
                                <div key={i} className="chat-phase">
                                    <span className="chat-phase-line" />
                                    <span className="chat-phase-text">{item.type === "gui-start" ? "🖥️ Browser Active" : "Browser Closed"}</span>
                                    <span className="chat-phase-line" />
                                </div>
                            );
                        }

                        if (item.type === "phase") {
                            return (
                                <div key={i} className="chat-phase">
                                    <span className="chat-phase-line" />
                                    <span className="chat-phase-text">{item.content}</span>
                                    <span className="chat-phase-line" />
                                </div>
                            );
                        }

                        if (item.type === "sub-agent") {
                            return (
                                <div key={i} className="chat-phase sub-agent-phase">
                                    <span className="chat-phase-line" />
                                    <span className="chat-phase-text">🔀 {item.content}</span>
                                    <span className="chat-phase-line" />
                                </div>
                            );
                        }

                        if (item.type === "finding") {
                            const isClean = item.content.includes("✓") || item.content.includes("clean");
                            return <div key={i} className={`chat-finding ${isClean ? "clean" : "issue"}`}><Markdown content={item.content} /></div>;
                        }

                        if (item.type === "report") {
                            return (
                                <div key={i} className="chat-bubble agent report">
                                    <div className="chat-bubble-avatar">📋</div>
                                    <div className="chat-bubble-content"><Markdown content={item.content} /></div>
                                </div>
                            );
                        }

                        if (item.type === "log-group" && item.logs) {
                            const isExpanded = expandedGroups.has(i);
                            return (
                                <div key={i} className="chat-log-group">
                                    <button className="chat-log-toggle" onClick={() => toggleGroup(i)}>
                                        <svg className={`chevron ${isExpanded ? "open" : ""}`} width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polyline points="9 18 15 12 9 6" /></svg>
                                        <span>{item.logs.length} system logs</span>
                                    </button>
                                    {isExpanded && (
                                        <div className="chat-log-expanded">
                                            {item.logs.map((log, j) => (
                                                <div key={j} className={`log-line log-${log.level}`}>
                                                    <span className="log-line-level">{log.level.toUpperCase()}</span>
                                                    <span className="log-line-msg">{log.message}</span>
                                                </div>
                                            ))}
                                        </div>
                                    )}
                                </div>
                            );
                        }
                        return null;
                    })}

                    {status === "running" && (
                        <div className="chat-typing"><div className="typing-dot" /><div className="typing-dot" /><div className="typing-dot" /></div>
                    )}
                    <div ref={endRef} />
                </div>
            </div>

            {/* Chat Input */}
            <div className="chat-input-bar">
                <textarea
                    ref={inputRef}
                    className="chat-input"
                    value={inputValue}
                    onChange={(e) => setInputValue(e.target.value)}
                    placeholder={status === "running" ? "Send a message to the agent..." : "Continue the conversation..."}
                    onKeyDown={(e) => {
                        if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleSend(); }
                    }}
                    rows={1}
                />
                <button className="btn-chat-send" onClick={handleSend} disabled={!inputValue.trim()}>
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <line x1="22" y1="2" x2="11" y2="13" /><polygon points="22 2 15 22 11 13 2 9 22 2" />
                    </svg>
                </button>
            </div>
        </div>
    );
}
