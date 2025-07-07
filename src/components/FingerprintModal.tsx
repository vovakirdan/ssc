import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

type Props = { 
  onConfirm: () => void; 
  onCancel: () => void; 
};

export default function FingerprintModal({ onConfirm, onCancel }: Props) {
  const [localFp, setLocalFp]   = useState<string>();
//   const [remoteFp, setRemoteFp] = useState<string>();
  const [checked, setChecked]   = useState(false);

  // запрашиваем свой и удалённый отпечаток (одинаковые)
  useEffect(() => {
    invoke<string>("get_fingerprint").then(setLocalFp).catch(console.error);
    // remoteFp = localFp;   // здесь они одинаковые, но можно получить иначе
  }, []);

  return (
    <div className="modal-back">
      <div className="modal">
        <h2>Verify fingerprints</h2>

        <p>Your screen should show <b>{localFp ?? "…"}</b><br/>
           It must match on both devices.</p>

        <label style={{marginTop:12}}>
          <input type="checkbox" checked={checked}
                 onChange={e=>setChecked(e.target.checked)}/>
          &nbsp;I see the same code on both sides
        </label>

        <div style={{display: 'flex', gap: 12, marginTop: 16, justifyContent: 'center'}}>
          <button className="main-btn" style={{background: '#d32f2f'}}
                  onClick={onCancel}>
            Cancel
          </button>
          <button className="main-btn"
                  disabled={!checked} onClick={onConfirm}>
            Continue
          </button>
        </div>
      </div>
    </div>
  );
} 