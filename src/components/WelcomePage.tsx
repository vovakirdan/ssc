import { QrCode } from "lucide-react";
import ScanButton from "./ScanButton";
import UploadQRButton from "./UploadQRButton"; // Новый компонент

type Props = {
  onShowQR: () => void;
};

export default function WelcomePage({ onShowQR }: Props) {
  return (
    <div style={{ textAlign: "center", marginTop: 48 }}>
      <h1 style={{ fontWeight: 700 }}>Super Secret Chat</h1>
      <p>
        Welcome! <br />
        Start a secret P2P chat without any servers.
      </p>
      <div
        style={{
          margin: "32px 0",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 18,
        }}
      >
        <button
          onClick={onShowQR}
          className="main-btn"
          style={{
            fontSize: 22,
            padding: "12px 32px",
            display: "flex",
            alignItems: "center",
            gap: 10,
            width: 280,
            justifyContent: "center",
          }}
        >
          <QrCode size={26} /> Generate QR for Connection
        </button>
        <ScanButton />
        <UploadQRButton />
      </div>
    </div>
  );
}
