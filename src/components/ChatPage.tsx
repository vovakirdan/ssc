type Props = {
  onDisconnect: () => void;
};

export default function ChatPage({ onDisconnect }: Props) {
  return (
    <div style={{ padding: 32, textAlign: "center" }}>
      <h2>🔒 Chat connected!</h2>
      <p>(Messaging logic will be implemented next — WebRTC & crypto)</p>
      <button className="main-btn" onClick={onDisconnect} style={{ marginTop: 24 }}>
        Disconnect
      </button>
    </div>
  );
}
