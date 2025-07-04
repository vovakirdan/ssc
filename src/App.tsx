import { useState } from "react";
import WelcomePage from "./components/WelcomePage";
import QRCodeBlock from "./components/QRCodeBlock";
import ChatPage from "./components/ChatPage";

export type AppMode = "welcome" | "qr" | "chat";

export default function App() {
  const [mode, setMode] = useState<AppMode>("welcome");

  return (
    <main className="container">
      {mode === "welcome" && (
        <WelcomePage
          onShowQR={() => setMode("qr")}
          onConnected={() => setMode("chat")}
        />
      )}

      {mode === "qr" && (
        <QRCodeBlock onBack={() => setMode("welcome")} />
      )}

      {mode === "chat" && <ChatPage onDisconnect={() => setMode("welcome")} />}
    </main>
  );
}
