import { useState, useRef, useEffect } from "react";
import { QRCodeSVG }        from "qrcode.react";
import { ArrowLeft, Copy, Download } from "lucide-react";

type Props = {
  answer: string;
  onBack: () => void;      // ← назад просто в welcome
};

export default function AnswerQRCodeBlock({ answer, onBack }: Props) {
  const [sec, setSec]  = useState(600);          // 10 мин TTL
  const qrRef          = useRef<SVGSVGElement>(null);

  /* таймер */
  useEffect(()=>{
    const t = setInterval(()=>setSec(s=>s-1),1000);
    return ()=>clearInterval(t);
  },[]);

  /* copy */
  const copy = ()=>navigator.clipboard.writeText(answer);

  /* save png */
  const save = ()=>{
    if(!qrRef.current) return;
    const svg = new XMLSerializer().serializeToString(qrRef.current);
    const img = new Image();
    img.src = "data:image/svg+xml;base64,"+btoa(unescape(encodeURIComponent(svg)));
    img.onload=()=>{
      const c=document.createElement("canvas"); c.width=c.height=360;
      const ctx=c.getContext("2d")!; ctx.fillStyle="#fff"; ctx.fillRect(0,0,360,360); ctx.drawImage(img,0,0);
      const link=document.createElement("a"); link.download="answer.png"; link.href=c.toDataURL("image/png");
      link.click();
    };
  };

  return (
    <div style={{textAlign:"center",marginTop:32}}>
      <button onClick={onBack} style={{position:"absolute",left:24,top:24,background:"none",border:"none"}}>
        <ArrowLeft size={30}/>
      </button>

      <h2>Answer QR</h2>
      <p>Let the sender scan this code. TTL <b>{sec}s</b></p>

      <QRCodeSVG ref={qrRef} value={answer} size={360}/>

      <div style={{marginTop:16,display:"flex",gap:10,justifyContent:"center"}}>
        <button className="main-btn" onClick={save}><Download size={16}/> Save</button>
        <button className="main-btn" onClick={copy}><Copy size={16}/> Copy</button>
      </div>

      <textarea readOnly value={answer}
                style={{marginTop:16,width:330,height:120,fontSize:12,resize:"none"}}/>
    </div>
  );
}
