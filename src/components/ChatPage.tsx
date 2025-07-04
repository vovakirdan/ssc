import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type Props = { onDisconnect: () => void };

interface Msg { fromMe: boolean; text: string }

export default function ChatPage({ onDisconnect }: Props) {
  const [text, setText] = useState("");
  const [log, setLog] = useState<Msg[]>([]);

  // входящие события
  useEffect(() => {
    const un = listen<string>("ssc-msg", e => {
      setLog(l => [...l, { fromMe: false, text: e.payload }]);
    });
    return () => { un.then(f => f()); };
  }, []);

  async function send() {
    if (!text.trim()) return;
    const ok = await invoke<boolean>("send_text", { msg: text });
    if (ok) setLog(l => [...l, { fromMe: true, text }]);
    else alert("not connected");
    setText("");
  }

  return (
    <div style={{ padding: 24 }}>
      <h2>🔒 Connected – start chatting</h2>

      <div style={{ height: 280, overflowY: "auto", border: "1px solid #ccc", padding: 8, marginBottom: 12 }}>
        {log.map((m, i) => (
          <div key={i} style={{ textAlign: m.fromMe ? "right" : "left", margin: "4px 0" }}>
            <span style={{ background: m.fromMe ? "#4caf50" : "#2196f3", color: "#fff", padding: "6px 10px", borderRadius: 8 }}>
              {m.text}
            </span>
          </div>
        ))}
      </div>

      <form onSubmit={e => { e.preventDefault(); send(); }} style={{ display: "flex", gap: 8 }}>
        <input
          style={{ flex: 1, padding: 8, borderRadius: 6, border: "1px solid #ccc" }}
          value={text}
          onChange={e => setText(e.target.value)}
          placeholder="Type message…"
        />
        <button className="main-btn" style={{ padding: "0 16px" }}>Send</button>
        <button className="main-btn" style={{ background: "#d32f2f", padding: "0 16px" }} onClick={onDisconnect} type="button">
          Disconnect
        </button>
      </form>
    </div>
  );
}
