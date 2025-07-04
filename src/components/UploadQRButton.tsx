import { useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ImageUp } from "lucide-react";
import QrcodeDecoder from "qrcode-decoder";

type Props = {
  onConnected: () => void;
};

export default function UploadQRButton({ onConnected }: Props) {
  const inputRef = useRef<HTMLInputElement>(null);

  async function handleFileChange(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = async ev => {
      const url = ev.target?.result as string;
      try {
        const qr = new QrcodeDecoder();
        const result = await qr.decodeFromImage(url);
        if (result) {
          const ok = await invoke<boolean>("accept_answer", { encoded: result.data });
          if (ok) onConnected();
          else alert("Invalid or expired QR-code.");
        }
      } catch {
        alert("Could not recognize a QR-code in the image.");
      }
    };
    reader.readAsDataURL(file);
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
        <ImageUp size={22} /> Upload QR from file
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
