import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Camera } from "lucide-react";
import QRScanModal from "./ScanQRModal";

type Props = {
  onConnected: () => void;
};

export default function ScanButton({ onConnected }: Props) {
  const [showScan, setShowScan] = useState(false);

  async function handleScanResult(data: string) {
    const ok = await invoke<boolean>("accept_answer", { encoded: data });
    if (ok) onConnected();
    else alert("Invalid or expired QR-code.");
  }

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
          width: 280,
          justifyContent: "center",
        }}
        onClick={() => setShowScan(true)}
      >
        <Camera size={22} />
        Scan QR
      </button>

      {showScan && (
        <QRScanModal
          onScan={handleScanResult}
          onClose={() => setShowScan(false)}
        />
      )}
    </>
  );
}
