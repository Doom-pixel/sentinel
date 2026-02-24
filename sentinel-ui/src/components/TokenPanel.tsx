import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface TokenInfo { id: string; scope: string; is_valid: boolean; }

export default function TokenPanel() {
  const [tokens, setTokens] = useState<TokenInfo[]>([]);
  useEffect(() => {
    const interval = setInterval(async () => { try { setTokens(await invoke<TokenInfo[]>("get_active_tokens")); } catch {} }, 2000);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="card token-panel">
      <h2>Active Capabilities</h2>
      <div className="token-list">
        {tokens.length === 0 && <div className="token-empty">No active tokens</div>}
        {tokens.map((t) => (
          <div key={t.id} className={`token-item ${t.is_valid ? "valid" : "expired"}`}>
            <div className="token-id">{t.id.slice(0, 12)}...</div>
            <div className="token-scope">{t.scope}</div>
            <span className={`token-badge ${t.is_valid ? "badge-active" : "badge-expired"}`}>
              {t.is_valid ? "ACTIVE" : "EXPIRED"}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
