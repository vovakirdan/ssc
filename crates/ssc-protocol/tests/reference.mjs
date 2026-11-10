// Independent test oracle using Node/OpenSSL. Never imported by application code.
import {createPrivateKey,createPublicKey,diffieHellman,createHash,hkdfSync,createCipheriv} from 'node:crypto';
const bytes=(n)=>Buffer.alloc(32,n);
const hash=(...v)=>createHash('sha256').update(Buffer.concat(v.map(x=>typeof x==='string'?Buffer.from(x):x))).digest();
const be=(n)=>{const b=Buffer.alloc(8);b.writeBigUInt64BE(BigInt(n));return b;};
const privateKey=(n)=>createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b656e04220420','hex'),bytes(n)]),format:'der',type:'pkcs8'});
export function reference(){
  const a=privateKey(9),b=privateKey(17);
  const ap=createPublicKey(a).export({format:'der',type:'spki'}).subarray(-32),bp=createPublicKey(b).export({format:'der',type:'spki'}).subarray(-32);
  const context=hash('ssc-chat-v4/session-context\0',be(Buffer.byteLength('wasm-native-vector')),'wasm-native-vector');
  const commit=(role,pub,nonce)=>hash('ssc-chat-v4/key-commitment\0',Buffer.from([4,role]),context,pub,nonce);
  const ca=commit(0,ap,bytes(11)),cb=commit(1,bp,bytes(19));
  const transcript=hash('ssc-chat-v4/transcript\0',Buffer.from([4]),context,Buffer.from([0]),ca,ap,bytes(11),Buffer.from([1]),cb,bp,bytes(19));
  const shared=diffieHellman({privateKey:a,publicKey:createPublicKey(b)});
  const keys=Buffer.from(hkdfSync('sha256',shared,transcript,'ssc-chat-v4/directional-keys',64));
  const sas=Buffer.from(hkdfSync('sha256',shared,transcript,'ssc-chat-v4/sas',32)).subarray(0,6).toString('hex');
  const encrypt=(key,kind,seq,payload)=>{
    const header=Buffer.concat([Buffer.from([4,kind]),be(seq)]),nonce=Buffer.concat([Buffer.alloc(4),be(seq)]);
    const cipher=createCipheriv('chacha20-poly1305',key,nonce,{authTagLength:16});cipher.setAAD(header,{plaintextLength:payload.length});
    return Buffer.concat([header,cipher.update(payload),cipher.final(),cipher.getAuthTag()]);
  };
  const chunk=Buffer.from(JSON.stringify({id:'07'.repeat(16),index:0,total:1,data:Buffer.from('Привет из WASM 🔒').toString('base64')}));
  const hex=v=>v.toString('hex');
  return {commit_a:hex(Buffer.concat([Buffer.from([4,0,0]),ca])),commit_b:hex(Buffer.concat([Buffer.from([4,0,1]),cb])),reveal_a:hex(Buffer.concat([Buffer.from([4,3,0]),ap,bytes(11)])),reveal_b:hex(Buffer.concat([Buffer.from([4,3,1]),bp,bytes(19)])),sas,ready_a:hex(encrypt(keys.subarray(0,32),1,1,Buffer.alloc(0))),ready_b:hex(encrypt(keys.subarray(32),1,1,Buffer.alloc(0))),text:hex(encrypt(keys.subarray(0,32),2,2,chunk)),event:hex(encrypt(keys.subarray(0,32),4,3,Buffer.from(JSON.stringify({type:"reaction",target:"07".repeat(16),emoji:"🔥"}))))};
}
