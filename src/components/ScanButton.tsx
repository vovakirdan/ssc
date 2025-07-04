import { Camera } from "lucide-react";

export default function ScanButton() {
  return (
    <button className="main-btn" style={{ fontSize: 20, padding: "10px 26px", display: "flex", alignItems: "center", gap: 10 }}>
      <Camera size={22} />
      Scan QR (coming soon)
    </button>
  );
}
