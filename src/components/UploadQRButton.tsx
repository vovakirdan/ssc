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

    console.log("File selected:", file.name, "Size:", file.size, "Type:", file.type);

    const reader = new FileReader();
    reader.onload = async ev => {
      const url = ev.target?.result as string;
      console.log("File loaded as data URL, length:", url.length);
      
      try {
        const qr = new QrcodeDecoder();
        console.log("QrcodeDecoder created");
        
        const result = await qr.decodeFromImage(url);
        console.log("QR decode result:", result);
        
        if (result && result.data) {
          console.log("QR data found:", result.data.substring(0, 50) + "...");
          
          try {
            // Создаем answer для полученного offer
            const answer = await invoke<string>("accept_offer_and_create_answer", { encoded: result.data });
            console.log("Answer created, length:", answer.length);
            
            // Устанавливаем answer для завершения соединения
            const success = await invoke<boolean>("set_answer", { encoded: answer });
            console.log("Connection result:", success);
            
            if (success) {
              onConnected();
            } else {
              alert("Failed to establish connection. Please try again.");
            }
          } catch (error) {
            console.error("Connection error:", error);
            alert("Invalid or expired QR-code. Please try again.");
          }
        } else {
          console.log("No QR data found in result");
          alert("Could not recognize a QR-code in the image.");
        }
      } catch (error) {
        console.error("QR decode error:", error);
        alert("Could not recognize a QR-code in the image.");
      }
    };
    
    reader.onerror = (error) => {
      console.error("FileReader error:", error);
      alert("Error reading file.");
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
