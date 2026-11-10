import type { ChatEvent } from '../../src/chat/types';
export interface ProtocolStep { outgoing: Uint8Array[]; sas: string | null; peer_ready: boolean; message: string | null; event: ChatEvent | null }
export interface ProtocolSession {
  start(): Uint8Array[];
  receive(frame: Uint8Array): ProtocolStep;
  sas(): string | null;
  peer_ready(): boolean;
  local_ready(): boolean;
  confirm(): Uint8Array;
  send_text(text: string, messageId: Uint8Array): Uint8Array[];
  send_event(event: string): Uint8Array;
  close(): void;
  free(): void;
}
export interface DecodedInvitation { session_id: string; sdp_type: 'offer' | 'answer'; sdp: string; ice_candidates: RTCIceCandidateInit[] }
export interface ProtocolModule {
  default(): Promise<unknown>;
  ProtocolSession: new (id: string, initiator: boolean, seed: Uint8Array, nonce: Uint8Array) => ProtocolSession;
  FileHasher: new () => { update(bytes: Uint8Array): void; finish(): string; abort(): void; free(): void };
  encode_invitation(id: string, sdpType: 'offer' | 'answer', sdp: string, candidates: string): string;
  decode_invitation(encoded: string): DecodedInvitation;
}
let loading: Promise<ProtocolModule> | undefined;
export function loadProtocol(): Promise<ProtocolModule> {
  if (!loading) loading = (async () => {
    const modulePath = import.meta.env.VITE_SSC_PROTOCOL_URL || '/protocol/ssc_protocol.js';
    if (!/^\/protocol\/(?:[a-f0-9]{64}\/)?ssc_protocol\.js$/.test(modulePath)) throw new Error('Invalid protocol module path');
    // Absolute same-origin URL avoids Vite dev's asset-import query rewriting.
    const moduleUrl = new URL(modulePath, location.origin).href;
    const module = await import(/* @vite-ignore */ moduleUrl) as ProtocolModule;
    await module.default();
    return module;
  })().catch(() => { loading = undefined; throw new Error('Protocol unavailable'); });
  return loading;
}
export function entropy(size: number): Uint8Array { return crypto.getRandomValues(new Uint8Array(size)); }
export function capability(): string { return btoa(String.fromCharCode(...entropy(32))).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', ''); }
export function connectionId(): string { return Array.from(entropy(16), byte => byte.toString(16).padStart(2, '0')).join(''); }
