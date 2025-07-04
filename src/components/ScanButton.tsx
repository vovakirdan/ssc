import { useState } from "react";
import { Camera } from "lucide-react";
import QRScanModal from "./ScanQRModal";

export default function ScanButton() {
  const [showScan, setShowScan] = useState(false);

  return (
    <>
      <button
        className="main-btn"
        style={{
          fontSize: 20,
          padding: "10px 26px",
          display: "flex",
          alignItems: "center",
          gap: 10,
        }}
        onClick={() => setShowScan(true)}
      >
        <Camera size={22} />
        Scan QR
      </button>
      {showScan && (
        <QRScanModal
          onScan={(data) => {
            alert("Scanned QR:\n" + data);
            // Можно здесь вызвать переход в окно подтверждения соединения
          }}
          onClose={() => setShowScan(false)}
        />
      )}
    </>
  );
}
