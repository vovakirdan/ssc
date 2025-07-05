import { useState } from "react";
import { invoke }   from "@tauri-apps/api/core";
import { X }        from "lucide-react";

type Props = {
  onConnected: () => void;
  onClose:     () => void;
};

export default function PasteAnswerModal({ onConnected,onClose }:Props){
  const [txt,setTxt]=useState("");

  async function handle(){
    try{
      const ok = await invoke<boolean>("set_answer",{ encoded:txt.trim() });
      if(ok) onConnected();
      else   alert("Failed to apply answer");
    }catch{ alert("Invalid answer string"); }
  }

  return(
    <div style={{position:"fixed",inset:0,background:"#0008",display:"flex",justifyContent:"center",alignItems:"center",zIndex:60}}>
      <div style={{background:"#fff",padding:24,borderRadius:12,width:380}}>
        <button onClick={onClose} style={{position:"absolute",top:12,right:16,background:"none",border:"none"}}><X size={28}/></button>
        <h3>Paste Answer string</h3>
        <textarea value={txt} onChange={e=>setTxt(e.target.value)} rows={6} style={{width:"100%",marginTop:8}}/>
        <button className="main-btn" style={{marginTop:12,width:"100%"}} onClick={handle}>Apply &amp; Connect</button>
      </div>
    </div>
  );
}
