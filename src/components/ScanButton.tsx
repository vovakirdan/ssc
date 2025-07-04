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
    try {
      // Создаем answer для полученного offer
      const answer = await invoke<string>("accept_offer_and_create_answer", { encoded: data });
      
      // Устанавливаем answer для завершения соединения
      const success = await invoke<boolean>("set_answer", { encoded: answer });
      
      if (success) {
        onConnected();
      } else {
        alert("Failed to establish connection. Please try again.");
      }
    } catch (error) {
      console.error("Connection error:", error);
      alert("Invalid or expired QR-code. Please try again.");
    }
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
