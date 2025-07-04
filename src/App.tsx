import { useState } from "react";
import WelcomePage from "./components/WelcomePage";
import QRCodeBlock from "./components/QRCodeBlock";

function App() {
  const [showQR, setShowQR] = useState(false);

  return (
    <main className="container">
      {!showQR ? (
        <WelcomePage onShowQR={() => setShowQR(true)} />
      ) : (
        <QRCodeBlock onBack={() => setShowQR(false)} />
      )}
    </main>
  );
}

export default App;
