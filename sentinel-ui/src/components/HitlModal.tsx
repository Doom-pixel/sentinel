interface ManifestInfo { id: string; action_description: string; parameters_json: string; risk_level: string; }
interface Props { manifest: ManifestInfo; onDecision: (manifestId: string, approved: boolean) => void; }

export default function HitlModal({ manifest, onDecision }: Props) {
  const riskClass = manifest.risk_level.toLowerCase();

  return (
    <div className="hitl-overlay">
      <div className="hitl-modal">
        <div className="hitl-header">
          <h2>Approval Required</h2>
          <span className={`hitl-risk ${riskClass}`}>{manifest.risk_level}</span>
        </div>
        <div className="hitl-body">
          <div className="hitl-field">
            <label>Manifest</label>
            <code>{manifest.id}</code>
          </div>
          <div className="hitl-field">
            <label>Action</label>
            <p>{manifest.action_description}</p>
          </div>
          <div className="hitl-field">
            <label>Parameters</label>
            <pre>{manifest.parameters_json}</pre>
          </div>
        </div>
        <div className="hitl-actions">
          <button className="btn-reject" onClick={() => onDecision(manifest.id, false)}>Reject</button>
          <button className="btn-approve" onClick={() => onDecision(manifest.id, true)}>Approve</button>
        </div>
      </div>
    </div>
  );
}
