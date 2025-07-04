import { useRef } from "react";
import { ImageUp } from "lucide-react";
import QrcodeDecoder from "qrcode-decoder";

export default function UploadQRButton() {
  const inputRef = useRef<HTMLInputElement>(null);

  async function handleFileChange(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = async function (event) {
      const url = event.target?.result as string;
      if (!url) return;
      const qr = new QrcodeDecoder();
      try {
        const result = await qr.decodeFromImage(url);
        if (result) {
          alert("Scanned QR from file:\n" + result.data);
        }
        // Можно интегрировать дальше по логике чата
      } catch (err) {
        alert("Could not recognize QR code in image.");
      }
    };
    reader.readAsDataURL(file);
    // Сброс input, чтобы повторная загрузка работала
    if (inputRef.current) inputRef.current.value = "";
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
        onClick={() => inputRef.current?.click()}
      >
        <ImageUp size={22} />
        Upload QR from file
      </button>
      <input
        ref={inputRef}
        type="file"
        accept="image/*"
        onChange={handleFileChange}
        style={{ display: "none" }}
      />
    </>
  );
}
