import { useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ImageUp } from "lucide-react";
import QrcodeDecoder from "qrcode-decoder";

type Props = { onAnswerReady: (answer: string) => void };

export default function UploadQRButton({ onAnswerReady }: Props) {
  const inp = useRef<HTMLInputElement>(null);

  async function handleFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]; if(!file) return;
    const url  = await file2url(file);

    const result = await new QrcodeDecoder().decodeFromImage(url);
    if (!result) return alert("No QR found");
    const data = result.data;

    try{
      const ans = await invoke<string>("accept_offer_and_create_answer",{ encoded:data });
      onAnswerReady(ans);
    }catch{ alert("Invalid offer QR"); }
    e.target.value = "";
  }

  return (
    <>
      <button className="main-btn" style={{fontSize:20,padding:"10px 26px"}} onClick={()=>inp.current?.click()}>
        <ImageUp size={22}/> Upload Offer
      </button>
      <input ref={inp} type="file" accept="image/*" hidden onChange={handleFile}/>
    </>
  );
}

function file2url(f:File):Promise<string>{
  return new Promise(r=>{
    const fr=new FileReader();
    fr.onload=e=>r(e!.target!.result as string);
    fr.readAsDataURL(f);
  });
}
