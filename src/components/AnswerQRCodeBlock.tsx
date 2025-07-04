import { QRCodeSVG } from "qrcode.react";
import { useState, useRef, useEffect } from "react";
import { ArrowLeft } from "lucide-react";

type Props = {
  answer: string;
  onBack: () => void;
};

export default function AnswerQRCodeBlock({ answer, onBack }: Props) {
  const [seconds, setSeconds] = useState(120);         // 2-мин TTL
  const qrRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    const t = setInterval(() => setSeconds(s => s - 1), 1000);
    return () => clearInterval(t);
  }, []);

  return (
    <div style={{ textAlign: "center", marginTop: 32 }}>
      <button onClick={onBack} style={{ position:"absolute",left:24,top:24,background:"none",border:"none" }}>
        <ArrowLeft size={30}/>
      </button>

      <h2>Answer QR — give it back to Sender</h2>
      <p>Expires in <b>{seconds}s</b></p>
      <QRCodeSVG ref={qrRef} value={answer} size={360}/>
      <p style={{marginTop:16,color:"#666"}}>Let инициатор отсканирует этот код.<br/>
        После этого окно можно закрыть и перейти в Chat.</p>
    </div>
  );
}
