import { useState } from "react";
import WelcomePage         from "./components/WelcomePage";
import QRCodeBlock         from "./components/QRCodeBlock";
import AnswerQRCodeBlock   from "./components/AnswerQRCodeBlock";
import ChatPage            from "./components/ChatPage";

type Mode = "welcome" | "offer" | "answer" | "chat";

export default function App() {
  const [mode,  setMode ] = useState<Mode>("welcome");
  const [answer,setAnswer] = useState<string>("");

  return (
    <main className="container">
      {mode==="welcome" && <WelcomePage
                              onShowOffer={()=>setMode("offer")}
                              showAnswerQR={ans=>{setAnswer(ans);setMode("answer");}}
                          />}
      {mode==="offer"   && <QRCodeBlock  onBack={()=>setMode("welcome")}/>}
      {mode==="answer"  && <AnswerQRCodeBlock answer={answer} onBack={()=>setMode("chat")}/>}
      {mode==="chat"    && <ChatPage onDisconnect={()=>setMode("welcome")}/>}
    </main>
  );
}
