import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { QRCodeSVG } from "qrcode.react";
import { ArrowLeft, Download, RefreshCw } from "lucide-react";

type Props = {
  onBack: () => void;
};

const QR_TTL = 60;

export default function QRCodeBlock({ onBack }: Props) {
  const [offer, setOffer] = useState<string | null>(null);
  const [seconds, setSeconds] = useState(QR_TTL);
  const qrRef = useRef<SVGSVGElement>(null);

  // Генерация QR
  const generateOffer = useCallback(async () => {
    const result = await invoke<string>("generate_offer", {});
    console.log("Generated QR data length:", result.length);
    console.log("QR data preview:", result.substring(0, 100) + "...");
    setOffer(result);
    setSeconds(QR_TTL);
  }, []);

  // Сохранение QR кода как изображение
  const saveQRCode = useCallback(() => {
    if (!qrRef.current || !offer) {
      console.log("QR code not ready for saving");
      return;
    }

    try {
      const svg = qrRef.current;
      const svgData = new XMLSerializer().serializeToString(svg);
      const canvas = document.createElement("canvas");
      const ctx = canvas.getContext("2d");
      const img = new Image();

      canvas.width = 360;
      canvas.height = 360;

      img.onload = () => {
        if (ctx) {
          // White background
          ctx.fillStyle = "white";
          ctx.fillRect(0, 0, canvas.width, canvas.height);
          ctx.drawImage(img, 0, 0);
          
          // Try to use native file dialog if available
          if (navigator.canShare && navigator.canShare({ files: [] })) {
            // Mobile sharing API
            canvas.toBlob((blob) => {
              if (blob) {
                const file = new File([blob], `qr-code-${Date.now()}.png`, { type: 'image/png' });
                navigator.share({
                  title: 'QR Code',
                  text: 'Generated QR code for connection',
                  files: [file]
                }).catch(console.error);
              }
            }, 'image/png');
          } else {
            // Fallback to download link
            const link = document.createElement("a");
            const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
            link.download = `qr-code-${timestamp}.png`;
            link.href = canvas.toDataURL("image/png");
            
            console.log("Attempting to save file:", link.download);
            
            // Add link to DOM, click and remove
            document.body.appendChild(link);
            link.click();
            document.body.removeChild(link);
            
            console.log("File should be saved to Downloads folder");
          }
        }
      };

      img.onerror = (error) => {
        console.error("Error loading image:", error);
      };

      img.src = "data:image/svg+xml;base64," + btoa(unescape(encodeURIComponent(svgData)));
    } catch (error) {
      console.error("Error saving QR code:", error);
    }
  }, [offer]);

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
      {offer && (
        <div style={{ position: "relative", display: "inline-block" }}>
          <QRCodeSVG ref={qrRef} value={offer} size={360} />
        </div>
      )}
      <div style={{ margin: "20px 0" }}>
        <button 
          className="main-btn" 
          onClick={generateOffer}
          style={{ 
            background: "#2196F3", 
            border: "none", 
            borderRadius: "4px", 
            padding: "8px 16px", 
            cursor: "pointer",
            display: "inline-flex",
            alignItems: "center",
            gap: "8px",
            color: "white",
            fontSize: "14px"
          }}
        >
          <RefreshCw size={16} />
          Refresh QR
        </button>
        {offer && (
          <button 
            onClick={saveQRCode}
            style={{ 
              marginLeft: 10,
              background: "#4CAF50", 
              border: "none", 
              borderRadius: "4px", 
              padding: "8px 16px", 
              cursor: "pointer",
              display: "inline-flex",
              alignItems: "center",
              gap: "8px",
              color: "white",
              fontSize: "14px"
            }}
            title="Save QR code"
          >
            <Download size={16} />
            Save QR
          </button>
        )}
      </div>
    </div>
  );
}
