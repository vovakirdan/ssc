// Actual WASM execution; deterministic entropy and synthetic file content are public test fixtures.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {reference} from './reference.mjs';
import init, {ProtocolSession, FileHasher, decode_invitation, encode_invitation} from '../pkg/ssc_protocol.js';
await init({module_or_path:await fs.readFile(new URL('../pkg/ssc_protocol_bg.wasm',import.meta.url))});
const expected=JSON.parse(await fs.readFile(new URL('./vectors.json',import.meta.url),'utf8'));
assert.deepEqual(expected,reference(),'v4 fixture must match independent Node/OpenSSL implementation');
const hex=value=>Buffer.from(value).toString('hex');
const bytes=(value,n=32)=>new Uint8Array(n).fill(value);
const id='07'.repeat(16);
function sessions(){
  const a=new ProtocolSession('wasm-native-vector',true,bytes(9),bytes(11));
  const b=new ProtocolSession('wasm-native-vector',false,bytes(17),bytes(19));
  const ac=a.start()[0],bc=b.start()[0];assert.equal(hex(ac),expected.commit_a);assert.equal(hex(bc),expected.commit_b);
  const ar=a.receive(bc).outgoing[0],br=b.receive(ac).outgoing[0];assert.equal(hex(ar),expected.reveal_a);assert.equal(hex(br),expected.reveal_b);
  a.receive(br);b.receive(ar);assert.equal(a.sas(),expected.sas);assert.equal(b.sas(),expected.sas);
  return [a,b];
}
const [a,b]=sessions();
try {
  assert.equal(a.local_ready(),false);assert.equal(a.peer_ready(),false);
  assert.throws(()=>a.send_text('premature',bytes(7,16)),/verification_required/);
  assert.throws(()=>a.send_event(JSON.stringify({type:'receipt',id})),/verification_required/);
  const ra=a.confirm(),rb=b.confirm();assert.equal(hex(ra),expected.ready_a);assert.equal(hex(rb),expected.ready_b);a.receive(rb);b.receive(ra);
  const text=a.send_text('Привет из WASM 🔒',bytes(7,16))[0];assert.equal(hex(text),expected.text);
  const received=b.receive(text);assert.equal(received.message,'Привет из WASM 🔒');assert.deepEqual(received.event,{type:'text',id,text:received.message});
  const reaction={type:'reaction',target:id,emoji:'🔥'};const event=a.send_event(JSON.stringify(reaction));assert.equal(hex(event),expected.event);assert.deepEqual(b.receive(event).event,reaction);
  for(const event of [
    {type:'capabilities',audio:['audio/webm;codecs=opus','audio/mp4','audio/wav']},
    {type:'attachment_offer',id,name:'файл.dat',mime:'application/octet-stream',size:0,sha256:'ab'.repeat(32),kind:'file'},
    {type:'attachment_accept',id},{type:'attachment_reject',id},{type:'attachment_chunk',id,offset:0,data:Buffer.from([0,255,128]).toString('base64')},
    {type:'attachment_progress',id,received:3},{type:'attachment_complete',id},{type:'attachment_cancel',id},{type:'receipt',id},
    {type:'reaction',target:id,emoji:null},{type:'text',id,text:'small typed text'},
  ])assert.deepEqual(b.receive(a.send_event(JSON.stringify(event))).event,event);
  for(const event of [{type:'receipt',id,extra:true},{type:'reaction',target:id},{type:'attachment_offer',id,name:'x',mime:'x',size:104857601,sha256:'ab'.repeat(32),kind:'file'},{type:'capabilities',audio:['audio/ogg']},{type:'reaction',target:id,emoji:'💣'}])assert.throws(()=>a.send_event(JSON.stringify(event)),/invalid_event/);
  // Reassembled text may exceed the one-frame event limit and must still expose its ID.
  const large='🔒'.repeat(60_000);let assembled=null;
  for(const frame of a.send_text(large,bytes(8,16)))assembled=b.receive(frame).event??assembled;
  assert.deepEqual(assembled,{type:'text',id:'08'.repeat(16),text:large});
  const replay=a.send_event(JSON.stringify({type:'receipt',id}));b.receive(replay);assert.throws(()=>b.receive(replay),/replay/);assert.equal(b.sas(),undefined);
} finally {a.close();a.free();b.close();b.free();}
const [sender,receiver]=sessions();
try {
  receiver.receive(sender.confirm());sender.receive(receiver.confirm());
  const bye={type:'bye',reason:'expired'};const frame=sender.send_event(JSON.stringify(bye));assert.equal(sender.sas(),undefined);assert.deepEqual(receiver.receive(frame).event,bye);assert.equal(receiver.sas(),undefined);assert.throws(()=>sender.send_event(JSON.stringify({type:'receipt',id})),/session_closed/);
} finally {sender.free();receiver.free();}
const [tamperSender,tamperReceiver]=sessions();
try {
  tamperReceiver.receive(tamperSender.confirm());tamperSender.receive(tamperReceiver.confirm());
  const frame=tamperSender.send_event(JSON.stringify({type:'receipt',id}));frame[frame.length-1]^=1;
  assert.throws(()=>tamperReceiver.receive(frame),/authentication_failed/);assert.equal(tamperReceiver.sas(),undefined);
} finally {tamperSender.free();tamperReceiver.free();}
const oldPeer=new ProtocolSession('v3-rejected',true,bytes(1),bytes(2));
try {const old=new Uint8Array(35);old.set([3,0,1]);assert.throws(()=>oldPeer.receive(old),/handshake_context/);assert.throws(()=>oldPeer.start(),/session_closed/);}finally{oldPeer.free();}
const invite='v4:H4sIAAAAAAAE/6WOzQ5AMBCE32XPDou49MZriAOl1E+Llot4d0P6BrLJZna/ZGYuciTC8iQ4okORoLqRLUGvQXeqH/Q4zYux67Y7f5yg6qV58W/g4/YveNEI4/eGb8bMd0S6xU9aYzrptTUxqEfhOEkBJVRZ3Q9J1GFGwwAAAA==';
const decoded=decode_invitation(invite);assert.equal(decoded.session_id,'connection1');assert.equal(decoded.sdp_type,'offer');assert.deepEqual(decoded.ice_candidates,[]);
const recreated=decode_invitation(encode_invitation(decoded.session_id,'offer',decoded.sdp,JSON.stringify([{candidate:'candidate:1 1 udp 1 127.0.0.1 1 typ host',sdpMid:'0',sdpMLineIndex:0}])));
assert.equal(recreated.sdp,decoded.sdp);assert.equal(recreated.ice_candidates[0].sdpMLineIndex,0);assert.equal(recreated.ice_candidates[0].sdpMid,'0');
assert.throws(()=>new ProtocolSession('bad',true,bytes(1,31),bytes(2)),/invalid_entropy_length/);
assert.throws(()=>decode_invitation(invite.replace('v4:','v3:')),/unsupported_version/);
for(const size of [0,16*1024,100*1024*1024]){
  const hasher=new FileHasher(),overflow=size===100*1024*1024?new FileHasher():null,oracle=createHash('sha256'),block=bytes(0xa5,256*1024);
  try {
    for(let remaining=size;remaining;){const part=block.subarray(0,Math.min(remaining,block.length));hasher.update(part);overflow?.update(part);oracle.update(part);remaining-=part.length;}
    assert.equal(hasher.finish(),oracle.digest('hex'));assert.throws(()=>hasher.finish(),/hasher_finished/);assert.throws(()=>hasher.update(bytes(0,1)),/hasher_finished/);
    if(overflow){assert.throws(()=>overflow.update(bytes(0,1)),/file_size/);assert.throws(()=>overflow.finish(),/hasher_finished/);}
  } finally {hasher.free();overflow?.free();}
}
const aborted=new FileHasher();aborted.update(bytes(7,1024));aborted.abort();assert.throws(()=>aborted.finish(),/hasher_finished/);aborted.free();
console.log('PASS actual v4 WASM: independent crypto/event vectors, strict typed controls, IDs/large text, verification/replay/Bye retirement, v3 rejection, incremental 0/16KiB/100MiB SHA256');
