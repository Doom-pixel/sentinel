import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

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
  const [taskPrompt, setTaskPrompt] = useState("Audit this codebase for security vulnerabilities.");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const selectedProvider = providers.find((p) => p.id === provider);
  const needsKey = selectedProvider?.requires_key ?? false;

  useEffect(() => { invoke<ProviderInfo[]>("get_providers").then(setProviders); }, []);

  const handleLaunch = async () => {
    setLogs([]);
    setErrorMsg(null);
    setIsRunning(true);
    try {
      await invoke("start_agent", {
        provider,
        model,
        api_key: needsKey ? apiKey : null,
        target_directory: targetDirectory,
        task_prompt: taskPrompt,
      });
    } catch (e) {
      console.error("Launch failed:", e);
      setErrorMsg(String(e));
    } finally {
      setIsRunning(false);
    }
  };

  return (
    <div className="sidebar-section">
      <div className="section-title">Launch Agent</div>

      <div className="form-group">
        <label className="form-label">Target Directory</label>
        <input
          className="form-input"
          type="text"
          value={targetDirectory}
          onChange={(e) => setTargetDirectory(e.target.value)}
          placeholder="/path/to/project"
          disabled={isRunning}
        />
      </div>

      <div className="form-group">
        <label className="form-label">Task Prompt</label>
        <textarea
          className="form-textarea"
          value={taskPrompt}
          onChange={(e) => setTaskPrompt(e.target.value)}
          placeholder="Describe what the agent should do..."
          disabled={isRunning}
        />
      </div>

      <div className="form-group form-row">
        <div>
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
            {providers.map((p) => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
        </div>
        <div>
          <label className="form-label">Model</label>
          <input
            className="form-input"
            type="text"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            disabled={isRunning}
          />
        </div>
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

      {errorMsg && (
          <div className="form-group" style={{ color: "#ff4444", backgroundColor: "#3a1010", padding: "10px", borderRadius: "4px", fontSize: "0.9em", marginTop: "10px" }}>
              <b>Error:</b> {errorMsg}
          </div>
      )}

      <button
        className={`btn-launch ${isRunning ? "running" : ""}`}
        onClick={handleLaunch}
        disabled={isRunning}
      >
        {isRunning ? "Agent Running..." : "Launch Sentinel"}
      </button>
    </div>
  );
}
