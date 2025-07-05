import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { QRCodeSVG } from "qrcode.react";
import { ArrowLeft, Download, RefreshCw, Copy } from "lucide-react";

type Props = { onBack: () => void };

const QR_TTL = 6000;

export default function QRCodeBlock({ onBack }: Props) {
  const [offer, setOffer]   = useState<string | null>(null);
  const [sec,   setSec]     = useState(QR_TTL);
  const qrRef               = useRef<SVGSVGElement>(null);

  /** Generate Offer */
  const gen = useCallback(async ()=>{
    const s = await invoke<string>("generate_offer", {});
    setOffer(s); setSec(QR_TTL);
  },[]);

  /** copy */
  const copy = ()=>{ offer && navigator.clipboard.writeText(offer); };

  /** save png */
  const save = ()=>{
    if(!qrRef.current||!offer) return;
    const svgData=new XMLSerializer().serializeToString(qrRef.current);
    const img=new Image(); img.src="data:image/svg+xml;base64,"+btoa(unescape(encodeURIComponent(svgData)));
    img.onload=()=>{
      const c=document.createElement("canvas"); c.width=c.height=360;
      const ctx=c.getContext("2d")!; ctx.fillStyle="#fff"; ctx.fillRect(0,0,360,360); ctx.drawImage(img,0,0);
      const link=document.createElement("a"); link.download="offer.png"; link.href=c.toDataURL("image/png");
      link.click();
    };
  };

  /* timer */
  useEffect(()=>{
    if(!offer) return;
    if(sec===0){ gen(); return; }
    const t=setTimeout(()=>setSec(s=>s-1),1000); return ()=>clearTimeout(t);
  },[sec,offer,gen]);

  useEffect(()=>{ gen(); },[gen]);

  return (
    <div style={{textAlign:"center",marginTop:32}}>
      <button onClick={onBack} style={{position:"absolute",left:24,top:24,background:"none",border:"none"}}><ArrowLeft size={30}/></button>
      <h2>Offer QR</h2>
      <p>Scan within <b>{sec}s</b> or copy the text below.</p>

      {offer && <QRCodeSVG ref={qrRef} value={offer} size={360}/>}

      <div style={{marginTop:16,display:"flex",gap:10,justifyContent:"center"}}>
        <button className="main-btn" onClick={gen}><RefreshCw size={16}/> Refresh</button>
        {offer && <>
          <button className="main-btn" onClick={save}><Download size={16}/> Save</button>
          <button className="main-btn" onClick={copy}><Copy size={16}/> Copy</button>
        </>}
      </div>

      {offer && (
        <textarea readOnly value={offer}
          style={{marginTop:16,width:330,height:120,fontSize:12,resize:"none"}}/>
      )}
    </div>
  );
}
