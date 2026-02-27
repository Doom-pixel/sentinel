import type { ResourceLimits, NotificationConfig } from "../App";
 
 interface Props {
     resourceLimits: ResourceLimits;
     setResourceLimits: React.Dispatch<React.SetStateAction<ResourceLimits>>;
     notifications: NotificationConfig;
     setNotifications: React.Dispatch<React.SetStateAction<NotificationConfig>>;
 }
 
 export default function SettingsPanel({
     resourceLimits,
     setResourceLimits,
     notifications,
     setNotifications,
 }: Props) {
     const updateLimit = <K extends keyof ResourceLimits>(key: K, value: ResourceLimits[K]) => {
         setResourceLimits((prev) => ({ ...prev, [key]: value }));
     };
 
     const updateNotif = <K extends keyof NotificationConfig>(key: K, value: NotificationConfig[K]) => {
         setNotifications((prev) => ({ ...prev, [key]: value }));
     };
 
     return (
         <div className="settings-page">
             <h1 className="settings-title">Settings</h1>
             <p className="settings-subtitle">
                 Configure how agents operate and where to receive notifications.
             </p>
 
             {/* ── Agent Behavior ────────────────────────────────── */}
             <section className="settings-section">
                 <div className="settings-section-header">
                     <div className="settings-section-icon">
                         <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                             <path d="M12 2L2 7l10 5 10-5-10-5z" />
                             <path d="M2 17l10 5 10-5" />
                             <path d="M2 12l10 5 10-5" />
                         </svg>
                     </div>
                     <h2>Agent Behavior</h2>
                 </div>
                 <p className="settings-section-desc">
                     Control what agents are allowed to do inside their Docker container.
                 </p>
 
                 <div className="settings-grid">
                     {/* Autonomy Level */}
                     <div className="setting-card">
                         <div className="setting-card-header">
                             <label className="setting-label">Autonomy Level</label>
                         </div>
                         <div className="autonomy-options">
                             {([
                                 { value: "full", label: "Full Autonomy", desc: "Agent reads and writes files freely" },
                                 { value: "read_report", label: "Read & Report", desc: "Reads files, writes audit report only" },
                                 { value: "ask_write", label: "Ask Before Writing", desc: "Agent asks permission before any write" },
                                 { value: "read_only", label: "Read Only", desc: "No file modifications allowed" },
                             ] as const).map(opt => (
                                 <label key={opt.value} className={`autonomy-option ${resourceLimits.autonomyLevel === opt.value ? "selected" : ""}`}>
                                     <input
                                         type="radio"
                                         name="autonomy"
                                         value={opt.value}
                                         checked={resourceLimits.autonomyLevel === opt.value}
                                         onChange={() => updateLimit("autonomyLevel", opt.value)}
                                     />
                                     <div className="autonomy-option-content">
                                         <span className="autonomy-option-label">{opt.label}</span>
                                         <span className="autonomy-option-desc">{opt.desc}</span>
                                     </div>
                                 </label>
                             ))}
                         </div>
                     </div>
 
                     {/* Container Memory */}
                     <div className="setting-card">
                         <div className="setting-card-header">
                             <label className="setting-label">Container Memory</label>
                             <span className="setting-value-badge">{resourceLimits.maxMemoryMb} MB</span>
                         </div>
                         <input
                             type="range"
                             className="setting-range"
                             min={256}
                             max={4096}
                             step={256}
                             value={resourceLimits.maxMemoryMb}
                             onChange={(e) => updateLimit("maxMemoryMb", Number(e.target.value))}
                         />
                         <div className="setting-range-labels">
                             <span>256 MB</span>
                             <span>4 GB</span>
                         </div>
                     </div>
 
                     {/* Network Timeout */}
                     <div className="setting-card">
                         <div className="setting-card-header">
                             <label className="setting-label">Network Timeout</label>
                             <span className="setting-value-badge">{resourceLimits.networkTimeoutSecs}s</span>
                         </div>
                         <input
                             type="range"
                             className="setting-range"
                             min={10}
                             max={180}
                             step={10}
                             value={resourceLimits.networkTimeoutSecs}
                             onChange={(e) => updateLimit("networkTimeoutSecs", Number(e.target.value))}
                         />
                         <div className="setting-range-labels">
                             <span>10s</span>
                             <span>180s</span>
                         </div>
                     </div>
                 </div>
             </section>
 
             {/* ── Notifications ───────────────────────────── */}
             <section className="settings-section">
                 <div className="settings-section-header">
                     <div className="settings-section-icon">
                         <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                             <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
                             <path d="M13.73 21a2 2 0 0 1-3.46 0" />
                         </svg>
                     </div>
                     <h2>Notifications</h2>
                 </div>
                 <p className="settings-section-desc">
                     Get alerts when agents complete tasks or need attention.
                 </p>
 
                 <div className="settings-grid">
                     <div className="setting-card notif-card">
                         <div className="setting-card-header">
                             <label className="setting-label"><span className="notif-brand discord">Discord</span></label>
                         </div>
                         <input className="form-input" type="text" value={notifications.discordWebhookUrl} onChange={(e) => updateNotif("discordWebhookUrl", e.target.value)} placeholder="https://discord.com/api/webhooks/..." />
                     </div>
                     <div className="setting-card notif-card">
                         <div className="setting-card-header">
                             <label className="setting-label"><span className="notif-brand slack">Slack</span></label>
                         </div>
                         <input className="form-input" type="text" value={notifications.slackWebhookUrl} onChange={(e) => updateNotif("slackWebhookUrl", e.target.value)} placeholder="https://hooks.slack.com/services/..." />
                     </div>
                     <div className="setting-card notif-card">
                         <div className="setting-card-header">
                             <label className="setting-label"><span className="notif-brand telegram">Telegram</span></label>
                         </div>
                         <div className="setting-telegram-row">
                             <input className="form-input" type="password" value={notifications.telegramBotToken} onChange={(e) => updateNotif("telegramBotToken", e.target.value)} placeholder="Bot Token" />
                             <input className="form-input" type="text" value={notifications.telegramChatId} onChange={(e) => updateNotif("telegramChatId", e.target.value)} placeholder="Chat ID" />
                         </div>
                     </div>
                 </div>
             </section>
 
             {/* ── Community ──────────────────────────────────── */}
             <section className="settings-section">
                 <div className="settings-section-header">
                     <div className="settings-section-icon">
                         <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                             <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
                             <circle cx="9" cy="7" r="4" />
                             <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
                             <path d="M16 3.13a4 4 0 0 1 0 7.75" />
                         </svg>
                     </div>
                     <h2>Community</h2>
                 </div>
                 <p className="settings-section-desc">Join the community for support, updates, and feature requests.</p>
 
                 <div className="settings-grid">
                     <div className="setting-card">
                         <div className="setting-card-header">
                             <label className="setting-label"><span className="notif-brand discord">Discord Server</span></label>
                         </div>
                         <a
                             href="https://discord.gg/k967Q5q6xZ"
                             target="_blank"
                             rel="noopener noreferrer"
                             className="btn-discord-join"
                         >
                             <svg width="18" height="18" viewBox="0 0 127.14 96.36" fill="currentColor">
                                 <path d="M107.7,8.07A105.15,105.15,0,0,0,81.47,0a72.06,72.06,0,0,0-3.36,6.83A97.68,97.68,0,0,0,49,6.83,72.37,72.37,0,0,0,45.64,0,105.89,105.89,0,0,0,19.39,8.09C2.79,32.65-1.71,56.6.54,80.21h0A105.73,105.73,0,0,0,32.71,96.36,77.7,77.7,0,0,0,39.6,85.25a68.42,68.42,0,0,1-10.85-5.18c.91-.66,1.8-1.34,2.66-2a75.57,75.57,0,0,0,64.32,0c.87.71,1.76,1.39,2.66,2a68.68,68.68,0,0,1-10.87,5.19,77,77,0,0,0,6.89,11.1A105.25,105.25,0,0,0,126.6,80.22h0C129.24,52.84,122.09,29.11,107.7,8.07ZM42.45,65.69C36.18,65.69,31,60,31,53s5-12.74,11.43-12.74S54,46,53.89,53,48.84,65.69,42.45,65.69Zm42.24,0C78.41,65.69,73.25,60,73.25,53s5-12.74,11.44-12.74S96.23,46,96.12,53,91.08,65.69,84.69,65.69Z" />
                             </svg>
                             Join Discord
                         </a>
                     </div>
                 </div>
             </section>
 
             <div className="settings-footer">
                 <p>Sentinel v0.2.0 • Docker-Isolated Personal Agent</p>
             </div>
         </div>
     );
 }
