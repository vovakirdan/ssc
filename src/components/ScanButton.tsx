import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Camera } from "lucide-react";
import QRScanModal from "./ScanQRModal";

type Props = {
  onAnswerReady: (answer: string) => void;
};

export default function ScanButton({ onAnswerReady }: Props) {
  const [showScan, setShowScan] = useState(false);

  async function handleScan(offer: string) {
    try {
      const answer = await invoke<string>("accept_offer_and_create_answer", { encoded: offer });
      onAnswerReady(answer);
    } catch {
      alert("Invalid offer QR");
    }
  }

  return (
    <>
      <button className="main-btn" style={{fontSize:20,padding:"10px 26px"}} onClick={()=>setShowScan(true)}>
        <Camera size={22}/> Scan Offer
      </button>
      {showScan && <QRScanModal onScan={handleScan} onClose={()=>setShowScan(false)}/>}
    </>
  );
}
