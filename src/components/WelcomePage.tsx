import { useState } from "react";
import { QrCode } from "lucide-react";
import ScanButton        from "./ScanButton";
import UploadQRButton    from "./UploadQRButton";
import PasteOfferModal   from "./PasteOfferModal";

type Props={
  onShowOffer: ()=>void;
  showAnswerQR:(answer:string)=>void;
};

export default function WelcomePage({onShowOffer,showAnswerQR}:Props){
  const [paste,setPaste]=useState(false);
  return(
    <div style={{textAlign:"center",marginTop:48}}>
      <h1 style={{fontWeight:700}}>Super Secret Chat</h1>
      <p>Choose how to receive an offer, then hand back the answer.</p>

      <div style={{marginTop:32,display:"flex",flexDirection:"column",gap:18,alignItems:"center"}}>
        <button className="main-btn" style={{fontSize:22,padding:"12px 32px",display:"flex",gap:10}}
                onClick={onShowOffer}>
          <QrCode size={26}/> Generate Offer
        </button>

        <ScanButton     onAnswerReady={showAnswerQR}/>
        <UploadQRButton onAnswerReady={showAnswerQR}/>
        <button className="main-btn" onClick={()=>setPaste(true)}>Paste Offer string</button>
      </div>

      {paste && <PasteOfferModal onAnswerReady={ans=>{setPaste(false);showAnswerQR(ans);}}
                                onClose={()=>setPaste(false)}/> }
    </div>
  );
}
