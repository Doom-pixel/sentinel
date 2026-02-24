import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface ProviderInfo { id: string; name: string; requires_key: boolean; default_model: string; }
interface LogEntry { level: string; target: string; message: string; }
interface Props { isRunning: boolean; setIsRunning: (v: boolean) => void; setLogs: React.Dispatch<React.SetStateAction<LogEntry[]>>; }

export default function LaunchPanel({ isRunning, setIsRunning, setLogs }: Props) {
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [selectedProvider, setSelectedProvider] = useState("ollama");
  const [model, setModel] = useState("llama3.1:8b");
  const [apiKey, setApiKey] = useState("");
  const [modulePath, setModulePath] = useState("target/wasm32-wasip1/debug/sentinel_guest.wasm");
  const [allowRead, setAllowRead] = useState(".");
  const [allowWrite, setAllowWrite] = useState(".");

  useEffect(() => { invoke<ProviderInfo[]>("get_providers").then(setProviders); }, []);
  const selected = providers.find((p) => p.id === selectedProvider);

  const handleLaunch = async () => {
    setLogs([]);
    try {
      await invoke("start_agent", {
        modulePath, modelConfig: { provider: selectedProvider, model, api_key: apiKey || null },
        allowRead: allowRead.split(",").map((s) => s.trim()),
        allowWrite: allowWrite.split(",").map((s) => s.trim()),
      });
      setIsRunning(true);
    } catch (e) { console.error("Launch failed:", e); }
  };

  return (
    <div className="card launch-panel">
      <h2>Launch Agent</h2>
      <label>Provider</label>
      <select value={selectedProvider} onChange={(e) => { setSelectedProvider(e.target.value); const p = providers.find((p) => p.id === e.target.value); if (p) setModel(p.default_model); }} disabled={isRunning}>
        {providers.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
      </select>
      <label>Model</label>
      <input type="text" value={model} onChange={(e) => setModel(e.target.value)} disabled={isRunning} />
      {selected?.requires_key && (<><label>API Key</label><input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="sk-..." disabled={isRunning} /></>)}
      <label>Module Path</label>
      <input type="text" value={modulePath} onChange={(e) => setModulePath(e.target.value)} disabled={isRunning} />
      <div className="row-group">
        <div className="field-half"><label>Allow Read</label><input value={allowRead} onChange={(e) => setAllowRead(e.target.value)} disabled={isRunning} /></div>
        <div className="field-half"><label>Allow Write</label><input value={allowWrite} onChange={(e) => setAllowWrite(e.target.value)} disabled={isRunning} /></div>
      </div>
      <button className="btn-launch" onClick={handleLaunch} disabled={isRunning}>{isRunning ? "Running..." : "Launch Agent"}</button>
    </div>
  );
}
