import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface TokenInfo { id: string; scope: string; is_valid: boolean; }

export default function TokenPanel() {
  const [tokens, setTokens] = useState<TokenInfo[]>([]);
  useEffect(() => {
    const interval = setInterval(async () => {
      try { setTokens(await invoke<TokenInfo[]>("get_active_tokens")); } catch {}
    }, 2000);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="sidebar-section">
      <div className="section-title">Active Capabilities</div>
      <div className="token-list">
        {tokens.length === 0 && <div className="token-empty">No active tokens</div>}
        {tokens.map((t) => (
          <div key={t.id} className="token-item">
            <span className="token-id">{t.id.slice(0, 8)}</span>
            <span className="token-scope">{t.scope}</span>
            <span className={`token-status ${t.is_valid ? "active" : "expired"}`}>
              {t.is_valid ? "active" : "expired"}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
