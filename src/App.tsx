import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { QRCodeSVG } from "qrcode.react";
import "./App.css";

function App() {
  const [offer, setOffer] = useState<string | null>(null);
  const [qrTime, setQrTime] = useState<number | null>(null);

  // Генерируем offer и QR по нажатию
  async function generateOffer() {
    const result = await invoke<string>("generate_offer", {}); // Без параметров
    setOffer(result);
    setQrTime(Date.now()); // Сохраняем время генерации
  }

  // Проверяем TTL (60 секунд)
  const isQrActive =
    qrTime && Date.now() - qrTime < 60_000;

  return (
    <main className="container">
      <h1>Super Secret Chat (SSC)</h1>

      <button onClick={generateOffer}>Generate QR (Offer)</button>

      {offer && isQrActive && (
        <div style={{ marginTop: 24 }}>
          <p>Scan this QR within 60 seconds:</p>
          <QRCodeSVG value={offer} size={256} />
        </div>
      )}

      {offer && !isQrActive && (
        <div style={{ marginTop: 24 }}>
          <p style={{ color: "red" }}>QR-код устарел. Сгенерируйте новый.</p>
        </div>
      )}
    </main>
  );
}

export default App;
