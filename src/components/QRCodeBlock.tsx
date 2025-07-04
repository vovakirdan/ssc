import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { QRCodeSVG } from "qrcode.react";
import { ArrowLeft } from "lucide-react";

type Props = {
  onBack: () => void;
};

const QR_TTL = 60;

export default function QRCodeBlock({ onBack }: Props) {
  const [offer, setOffer] = useState<string | null>(null);
  const [seconds, setSeconds] = useState(QR_TTL);

  // Генерация QR
  const generateOffer = useCallback(async () => {
    const result = await invoke<string>("generate_offer", {});
    setOffer(result);
    setSeconds(QR_TTL);
  }, []);

  // Таймер обратного отсчёта
  useEffect(() => {
    if (offer === null) return;
    if (seconds === 0) {
      generateOffer();
      return;
    }
    const t = setTimeout(() => setSeconds((s) => s - 1), 1000);
    return () => clearTimeout(t);
  }, [seconds, offer, generateOffer]);

  // Первая генерация
  useEffect(() => {
    generateOffer();
  }, [generateOffer]);

  return (
    <div style={{ textAlign: "center", marginTop: 32 }}>
      <button onClick={onBack} style={{ position: "absolute", left: 24, top: 24, background: "none", border: "none", cursor: "pointer" }}>
        <ArrowLeft size={30} />
      </button>
      <h2>QR Code for Connection</h2>
      <p>Scan this QR within <b>{seconds}s</b></p>
      {offer && <QRCodeSVG value={offer} size={240} />}
      <div style={{ margin: "20px 0" }}>
        <button className="main-btn" onClick={generateOffer}>Refresh QR</button>
      </div>
    </div>
  );
}
