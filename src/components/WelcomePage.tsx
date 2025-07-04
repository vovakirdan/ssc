import { QrCode } from "lucide-react";
import ScanButton from "./ScanButton";
import UploadQRButton from "./UploadQRButton";

type Props = {
  onShowQR: () => void;
  onConnected: () => void;   // <- новый проп
};

export default function WelcomePage({ onShowQR, onConnected }: Props) {
  return (
    <div style={{ textAlign: "center", marginTop: 48 }}>
      <h1 style={{ fontWeight: 700 }}>Super Secret Chat</h1>
      <p>
        Welcome! <br />
        Start a secret P2P chat without any servers.
      </p>

      {/* кнопки */}
      <div
        style={{
          marginTop: 32,
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

        {/* сканирование с камеры */}
        <ScanButton onConnected={onConnected} />

        {/* альтернатива — загрузка файла */}
        <UploadQRButton onConnected={onConnected} />
      </div>
    </div>
  );
}
