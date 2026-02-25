import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface ProviderInfo { id: string; name: string; requires_key: boolean; default_model: string; }
interface LogEntry { level: string; target: string; message: string; }
interface Props {
    isRunning: boolean;
    setIsRunning: (v: boolean) => void;
    setLogs: React.Dispatch<React.SetStateAction<LogEntry[]>>;
}

export default function LaunchPanel({ isRunning, setIsRunning, setLogs }: Props) {
    const [providers, setProviders] = useState<ProviderInfo[]>([]);
    const [provider, setProvider] = useState("ollama");
    const [model, setModel] = useState("llama3.1:8b");
    const [apiKey, setApiKey] = useState("");
    const [targetDirectory, setTargetDirectory] = useState(".");
    const [taskPrompt, setTaskPrompt] = useState("");
    const [errorMsg, setErrorMsg] = useState<string | null>(null);

    const [showSettings, setShowSettings] = useState(false);
    const settingsRef = useRef<HTMLDivElement>(null);

    const selectedProvider = providers.find((p) => p.id === provider);
    const needsKey = selectedProvider?.requires_key ?? false;

    useEffect(() => { invoke<ProviderInfo[]>("get_providers").then(setProviders); }, []);

    useEffect(() => {
        function handleClickOutside(event: MouseEvent) {
            if (settingsRef.current && !settingsRef.current.contains(event.target as Node)) {
                setShowSettings(false);
            }
        }
        document.addEventListener("mousedown", handleClickOutside);
        return () => document.removeEventListener("mousedown", handleClickOutside);
    }, []);

    const handleLaunch = async () => {
        if (!taskPrompt.trim()) return;
        setLogs([]);
        setErrorMsg(null);
        setIsRunning(true);
        try {
            await invoke("start_agent", {
                provider,
                model,
                apiKey: needsKey ? apiKey : null,
                targetDirectory: targetDirectory,
                taskPrompt: taskPrompt,
            });
        } catch (e) {
            console.error("Launch failed:", e);
            setErrorMsg(String(e));
        } finally {
            setIsRunning(false);
        }
    };

    return (
        <div className="launch-panel">
            <h1 className="hero-title">What should Sentinel do today?</h1>

            <div className="search-box-container">
                <textarea
                    className="search-textarea"
                    value={taskPrompt}
                    onChange={(e) => setTaskPrompt(e.target.value)}
                    placeholder="E.g., Audit this codebase for security vulnerabilities..."
                    disabled={isRunning}
                    onKeyDown={(e) => {
                        if (e.key === 'Enter' && !e.shiftKey) {
                            e.preventDefault();
                            handleLaunch();
                        }
                    }}
                />

                <div className="search-actions">
                    <div className="search-actions-left" ref={settingsRef}>
                        <button
                            className="btn-icon"
                            onClick={() => setShowSettings(!showSettings)}
                            disabled={isRunning}
                            title="LLM Settings"
                        >
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path></svg>
                        </button>

                        <button
                            className="btn-target"
                            onClick={async () => {
                                try {
                                    const selected = await open({ directory: true, multiple: false });
                                    if (selected && typeof selected === "string") setTargetDirectory(selected);
                                } catch (err) {
                                    setErrorMsg(`Dialog Error: ${String(err)}`);
                                }
                            }}
                            disabled={isRunning}
                            title="Target Directory"
                        >
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
                            <span className="target-text">{targetDirectory === "." ? "Current Directory" : targetDirectory.split(/[\\/]/).pop()}</span>
                        </button>

                        {showSettings && (
                            <div className="settings-popover">
                                <div className="form-group">
                                    <label className="form-label">Provider</label>
                                    <select
                                        className="form-select"
                                        value={provider}
                                        onChange={(e) => {
                                            setProvider(e.target.value);
                                            const p = providers.find((p) => p.id === e.target.value);
                                            if (p) setModel(p.default_model);
                                        }}
                                        disabled={isRunning}
                                    >
                                        {providers.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
                                    </select>
                                </div>
                                <div className="form-group">
                                    <label className="form-label">Model</label>
                                    <input
                                        className="form-input"
                                        type="text"
                                        value={model}
                                        onChange={(e) => setModel(e.target.value)}
                                        disabled={isRunning}
                                    />
                                </div>
                                {needsKey && (
                                    <div className="form-group">
                                        <label className="form-label">API Key</label>
                                        <input
                                            className="form-input"
                                            type="password"
                                            value={apiKey}
                                            onChange={(e) => setApiKey(e.target.value)}
                                            placeholder="sk-..."
                                            disabled={isRunning}
                                        />
                                    </div>
                                )}
                            </div>
                        )}
                    </div>

                    <button
                        className={`btn-hero-launch ${isRunning ? "running" : ""}`}
                        onClick={handleLaunch}
                        disabled={isRunning || !taskPrompt.trim()}
                    >
                        {isRunning ? (
                            <div className="loader-dots"><span>.</span><span>.</span><span>.</span></div>
                        ) : (
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><line x1="22" y1="2" x2="11" y2="13"></line><polygon points="22 2 15 22 11 13 2 9 22 2"></polygon></svg>
                        )}
                    </button>
                </div>
            </div>

            {errorMsg && (
                <div className="error-banner">
                    <b>Error:</b> {errorMsg}
                </div>
            )}
        </div>
    );
}
