import { useState } from "react";
import QrReader from "react-qr-scanner";
import { X } from "lucide-react";

type Props = {
  onScan: (data: string) => void;
  onClose: () => void;
};

export default function QRScanModal({ onScan, onClose }: Props) {
  const [error, setError] = useState<string | null>(null);

  const handleScan = (result: { text?: string } | null) => {
    if (result && result.text) {
      onScan(result.text);
      onClose();
    }
  };

  const handleError = (err: any) => {
    setError("Camera error: " + err.message || String(err));
  };

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        background: "#181c2ecc",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        zIndex: 50,
        justifyContent: "center",
      }}
    >
      <button
        style={{
          position: "absolute",
          top: 16,
          right: 32,
          background: "none",
          border: "none",
          color: "#fff",
          fontSize: 28,
          cursor: "pointer",
        }}
        onClick={onClose}
      >
        <X size={36} />
      </button>
      <h2 style={{ color: "#fff", marginBottom: 16 }}>Scan QR</h2>
      <div style={{ width: 280, height: 280, background: "#222", borderRadius: 10 }}>
        <QrReader
          delay={200}
          style={{ width: "100%", height: "100%" }}
          onError={handleError}
          onResult={handleScan}
          constraints={{
            facingMode: "environment",
            width: { ideal: 1280 },
            height: { ideal: 720 }
          }}
        />
      </div>
      {error && (
        <div style={{ color: "tomato", marginTop: 12 }}>{error}</div>
      )}
    </div>
  );
}
