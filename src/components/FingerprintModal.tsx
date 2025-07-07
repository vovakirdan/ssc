import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type Props = { 
  onConfirm: () => void; 
  onCancel: () => void; 
};

export default function FingerprintModal({ onConfirm, onCancel }: Props) {
  const [localFp, setLocalFp]   = useState<string>();
//   const [remoteFp, setRemoteFp] = useState<string>();
  const [checked, setChecked]   = useState(false);

  // Ждем события подключения и запрашиваем fingerprint
  useEffect(() => {
    const un = listen("ssc-connected", () => {
      // Небольшая задержка для гарантии, что SAS установлен
      setTimeout(() => {
        invoke<string>("get_fingerprint").then(setLocalFp).catch(console.error);
      }, 200);
    });
    return () => { un.then(f => f()); };
  }, []);

  return (
    <div className="modal-back">
      <div className="modal">
        <h2>Сверьте отпечатки</h2>

        <p>Оба клиента должны показывать <b>{localFp ?? "…"}</b><br/>
           Этот код должен совпадать на обоих устройствах.<br/>
           Только тот, у кого этот код, может читать и отправлять сообщения.</p>

        <label style={{marginTop:12}}>
          <input type="checkbox" checked={checked}
                 onChange={e=>setChecked(e.target.checked)}/>
          &nbsp;Я вижу тот же код на обоих устройствах
        </label>

        <div style={{display: 'flex', gap: 12, marginTop: 16, justifyContent: 'center'}}>
          <button className="main-btn" style={{background: '#d32f2f'}}
                  onClick={onCancel}>
            Это не так. Коды не совпадают
          </button>
          <button className="main-btn"
                  disabled={!checked} onClick={onConfirm}>
            Продолжить
          </button>
        </div>
      </div>
    </div>
  );
} 