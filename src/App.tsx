import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event"
import WelcomePage         from "./components/WelcomePage";
import QRCodeBlock         from "./components/QRCodeBlock";
import AnswerQRCodeBlock   from "./components/AnswerQRCodeBlock";
import ChatPage            from "./components/ChatPage";

type Mode = "welcome" | "offer" | "answer" | "chat";

export default function App() {
  const [mode,  setMode ] = useState<Mode>("welcome");
  const [answer,setAnswer] = useState<string>("");

  useEffect(()=>{
    const un = listen("ssc-connected",()=>setMode("chat"));
    return ()=>{ un.then(f=>f()); };
  },[]);

  return (
    <main className="container">
      {mode==="welcome" && (
        <WelcomePage
          onShowOffer={()=>setMode("offer")}
          showAnswerQR={ans=>{setAnswer(ans);setMode("answer");}}
        />
      )}
      {mode==="offer" && (<QRCodeBlock onBack={()=>setMode("welcome")} onConnected={()=>setMode("chat")}/>)}
      {mode==="answer" && <AnswerQRCodeBlock  answer={answer} onBack={()=>setMode("welcome")}/>}
      {mode==="chat"   && <ChatPage           onDisconnect={()=>setMode("welcome")}/>}
    </main>
  );
}
