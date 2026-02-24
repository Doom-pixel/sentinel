interface ManifestInfo { id: string; action_description: string; parameters_json: string; risk_level: string; }
interface Props { manifest: ManifestInfo; onDecision: (manifestId: string, approved: boolean) => void; }

export default function HitlModal({ manifest, onDecision }: Props) {
  const riskColor = () => {
    switch (manifest.risk_level.toLowerCase()) {
      case "critical": return "var(--risk-critical)";
      case "high": return "var(--risk-high)";
      case "medium": return "var(--risk-medium)";
      default: return "var(--risk-low)";
    }
  };

  return (
    <div className="hitl-overlay">
      <div className="hitl-modal">
        <div className="hitl-header" style={{ borderColor: riskColor() }}>
          <div className="hitl-icon" style={{ background: riskColor() }}>!</div>
          <div>
            <h2>Pre-flight Verification</h2>
            <span className="hitl-risk" style={{ color: riskColor() }}>{manifest.risk_level.toUpperCase()} RISK</span>
          </div>
        </div>
        <div className="hitl-body">
          <div className="hitl-field"><label>Manifest ID</label><code>{manifest.id}</code></div>
          <div className="hitl-field"><label>Action</label><p>{manifest.action_description}</p></div>
          <div className="hitl-field"><label>Parameters</label><pre>{manifest.parameters_json}</pre></div>
        </div>
        <div className="hitl-actions">
          <button className="btn-reject" onClick={() => onDecision(manifest.id, false)}>Reject</button>
          <button className="btn-approve" onClick={() => onDecision(manifest.id, true)}>Approve &amp; Sign</button>
        </div>
      </div>
    </div>
  );
}
