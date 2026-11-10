import { ChatModel } from '../../src/chat/model';
import { waitForSendBuffer } from '../../src/chat/channel-buffer';
import type { ChatEvent, HashSink } from '../../src/chat/types';
import { playableVoiceFormats } from '../../src/chat/voice/recorder';
import { isCapability, RoomError, RoomsApi, type RoomAccess, type RoomMetadata } from './api';
import { capability, connectionId, entropy, loadProtocol, type ProtocolModule, type ProtocolSession } from './protocol';

export type Phase = 'home' | 'preview' | 'working' | 'waiting' | 'connecting' | 'verification' | 'waiting_peer' | 'chat' | 'recovering' | 'closed';
export interface Message { id: number; own: boolean; text: string }
export interface ChatView { phase: Phase; invite: string; sas: string | null; localConfirmed: boolean; peerConfirmed: boolean; remainingSeconds: number | null; messages: Message[]; error: string | null }
const empty = (phase: Phase): ChatView => ({ phase, invite: '', sas: null, localConfirmed: false, peerConfirmed: false, remainingSeconds: null, messages: [], error: null });
const pause = (milliseconds: number) => new Promise(resolve => setTimeout(resolve, milliseconds));
export function errorText(error: unknown): string {
  if (error instanceof Error && error.message === 'unsupported_version') return 'Версии клиентов несовместимы. Обновите desktop и перезагрузите web-страницу.';
  if (error instanceof RoomError) switch (error.code) {
    case 'room_unavailable': return 'Чат закрыт или срок ссылки истёк. Создайте новый чат.';
    case 'conflict': return 'Эта ссылка уже использована другим собеседником. Попросите новую ссылку.';
    case 'offer_pending': return 'Создатель ещё готовит подключение. Повторите вход через несколько секунд.';
    case 'rate_limited': return 'Слишком много запросов. Немного подождите и повторите попытку.';
    case 'capacity': return 'Сервис временно занят. Попробуйте позже.';
    case 'network': return 'Сервис недоступен. Проверьте сеть и повторите попытку.';
  }
  return 'Не удалось безопасно установить соединение. Создайте новый чат.';
}

export class BrowserChat {
  private view: ChatView;
  private listeners = new Set<(view: ChatView) => void>();
  private controller = new AbortController();
  private api = new RoomsApi(this.controller.signal);
  private room: RoomAccess | null = null;
  private joinDeadline: number | null = null;
  private joinAnchor: { server: number; mono: number } | null = null;
  private sessionAnchor: { server: number; mono: number } | null = null;
  private sessionDeadline: number | null = null;
  private firstCreateAttempt: number | null = null;
  private connectedAt: number | null = null;
  private protocol: ProtocolSession | null = null;
  private module: ProtocolModule | null = null;
  private pc: RTCPeerConnection | null = null;
  private channel: RTCDataChannel | null = null;
  private id = '';
  private inviteSecret = '';
  private createKey = '';
  private claimKey = '';
  private closed = false;
  private busy = false;
  private initiator = false;
  private remoteApplied = false;
  private sending: Promise<void> = Promise.resolve();
  private connectedPublished = false;
  private recovery: ReturnType<typeof setTimeout> | undefined;
  private handshake: ReturnType<typeof setTimeout> | undefined;
  private handshakeStarted = 0;
  private recoveryStarted = 0;
  private clock: ReturnType<typeof setInterval> | undefined;
  readonly conversation: ChatModel;
  private capabilitiesSent = false;
  private incomingBytes = 0;
  private incomingFrames = 0;
  constructor() {
    this.conversation = new ChatModel({
      isReady: () => !this.closed && !!this.protocol?.local_ready() && this.protocol.peer_ready() && this.channel?.readyState === 'open',
      send: event => this.sendEvent(event),
      createHasher: async () => {
        this.ensure(); const hasher = new this.module!.FileHasher(); let finished = false;
        return {
          update: async bytes => { this.ensure(); if (finished) throw new Error('Hasher closed'); hasher.update(bytes); },
          finish: async () => { try { this.ensure(); if (finished) throw new Error('Hasher closed'); return hasher.finish(); } finally { if (!finished) { finished = true; hasher.free(); } } },
          abort: () => { if (!finished) { finished = true; hasher.abort(); hasher.free(); } },
        } satisfies HashSink;
      },
      onPeerExit: reason => this.close(reason === 'expired' ? 'Время чата истекло. История на этом устройстве очищена.' : reason === 'exit' ? 'Собеседник вышел. История чата на этом устройстве очищена.' : 'Соединение прервано. История на этом устройстве очищена.'),
    });
    const match = /^\/r\/([A-Za-z0-9_-]{43})$/.exec(location.pathname);
    const secret = new URLSearchParams(location.hash.slice(1)).get('k');
    this.view = empty(match && isCapability(secret) ? 'preview' : 'home');
    if (match && isCapability(secret)) { this.id = match[1]; this.inviteSecret = secret; }
    else if (match) { this.view.error = 'Сессия не сохраняется после обновления. Откройте исходную ссылку или создайте новый чат.'; }
    // The bearer invitation stays in memory; it must not remain in browser history.
    if (location.hash || location.search) history.replaceState(null, '', location.pathname);
  }
  snapshot(): ChatView { return this.view; }
  subscribe(listener: (view: ChatView) => void): () => void { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; }
  private update(patch: Partial<ChatView>) {
    if (this.closed && patch.phase !== 'closed') return;
    this.view = { ...this.view, ...patch }; for (const listener of this.listeners) listener(this.view);
  }
  private ensure() {
    const deadline = this.sessionDeadline ?? this.joinDeadline;
    if (!this.closed && deadline !== null && performance.now() >= deadline) this.close('Время чата истекло. История на этом устройстве очищена.', false, 'expired');
    if (!this.closed && this.handshakeStarted && !this.protocol?.sas() && performance.now() - this.handshakeStarted >= 20_000) this.close('Обмен ключами не завершился. Создайте новый чат.');
    if (this.closed) throw new Error('Session closed');
  }
  private setMetadata(value: RoomMetadata) {
    this.ensure(); if (this.room && value.id !== this.room.id) throw new RoomError('invalid_response');
    const now = performance.now();
    const joinExpiry = Date.parse(value.join_expires_at);
    if (this.joinAnchor === null) {
      const remaining = Math.max(0, Math.min(300_000, joinExpiry - Date.now()));
      this.joinAnchor = { server: joinExpiry, mono: Math.min(now + remaining, this.firstCreateAttempt === null ? Infinity : this.firstCreateAttempt + 300_000) };
      this.joinDeadline = this.joinAnchor.mono;
    } else if (this.sessionDeadline === null && joinExpiry < this.joinAnchor.server) {
      this.joinDeadline = Math.min(this.joinDeadline!, this.joinAnchor.mono + joinExpiry - this.joinAnchor.server);
    }
    if (value.expires_at !== null) {
      const expiry = Date.parse(value.expires_at);
      if (this.sessionAnchor === null) {
        const remaining = Math.max(0, Math.min(3600_000, expiry - Date.now()));
        this.sessionAnchor = { server: expiry, mono: now + remaining };
      }
      // Convert wall time once. Identical/later metadata cannot renew or shorten on a clock correction.
      const serverDeadline = this.sessionAnchor.mono + Math.min(0, expiry - this.sessionAnchor.server);
      this.sessionDeadline = Math.min(this.sessionDeadline ?? Infinity, serverDeadline, this.joinDeadline! + 3600_000, (this.connectedAt ?? now) + 3600_000);
      this.updateCountdown();
    }
    this.ensure();
  }
  private updateCountdown() {
    if (this.sessionDeadline !== null) this.update({ remainingSeconds: Math.max(0, Math.ceil((this.sessionDeadline - performance.now()) / 1000)) });
  }
  private startClock() {
    this.clock = setInterval(() => { try { this.ensure(); this.updateCountdown(); } catch { /* ensure retires an expired session. */ } }, 1000);
  }
  async create(): Promise<void> {
    if (this.busy || this.closed || this.room) return;
    this.busy = true; this.update({ phase: 'working', error: null });
    try {
      this.firstCreateAttempt ??= performance.now();
      this.createKey ||= capability();
      const room = await this.api.create(this.createKey); this.ensure();
      this.room = { id: room.id, token: room.creator_token }; this.setMetadata(room);
      this.initiator = true; this.startClock();
      history.replaceState(null, '', `/r/${room.id}`);
      await this.prepare(true, connectionId()); this.ensure();
      const offer = await this.localDescription('offer'); this.ensure();
      this.setMetadata(await this.api.offer(this.room, offer));
      this.update({ phase: 'waiting', invite: room.invitation_url });
      void this.poll();
    } catch (error) {
      if (this.room) this.close(errorText(error));
      else this.update({ phase: 'home', error: errorText(error) });
    } finally { this.busy = false; }
  }
  async join(): Promise<void> {
    if (this.busy || this.closed || this.room || !this.inviteSecret) return;
    this.busy = true; this.update({ phase: 'working', error: null });
    try {
      this.claimKey ||= capability();
      const room = await this.api.join(this.id, this.inviteSecret, this.claimKey); this.ensure();
      this.room = { id: room.id, token: room.guest_token }; this.setMetadata(room); this.startClock();
      this.module = await loadProtocol(); this.ensure();
      const offer = this.module.decode_invitation(room.offer);
      if (offer.sdp_type !== 'offer') throw new Error('Invalid invitation');
      await this.prepare(false, offer.session_id); this.ensure();
      await this.applyRemote(room.offer, 'offer'); this.ensure();
      const answer = await this.localDescription('answer'); this.ensure();
      this.setMetadata(await this.api.answer(this.room, answer));
      this.update({ phase: 'connecting' });
      void this.poll();
    } catch (error) {
      if (this.room) this.close(errorText(error));
      else this.update({ phase: 'preview', error: errorText(error) });
    } finally { this.busy = false; }
  }
  private async prepare(initiator: boolean, id: string) {
    const module = this.module ?? await loadProtocol(); this.ensure();
    const servers = await this.api.turn(this.room!); this.ensure();
    this.module = module; this.id = id;
    const seed = entropy(32), nonce = entropy(32);
    try { this.protocol = new module.ProtocolSession(id, initiator, seed, nonce); }
    finally { seed.fill(0); nonce.fill(0); }
    this.pc = new RTCPeerConnection({ iceServers: servers, bundlePolicy: 'max-bundle' });
    const pc = this.pc;
    pc.onconnectionstatechange = () => {
      if (this.closed) return;
      if (pc.connectionState === 'connected') {
        this.connectedAt ??= performance.now();
        if (this.recoveryStarted && performance.now() - this.recoveryStarted >= 10_000) { this.close('Связь потеряна. Создайте новый чат.'); return; }
        clearTimeout(this.recovery); this.recovery = undefined; this.recoveryStarted = 0;
        this.startHandshakeDeadline(); void this.enqueue(() => this.updateCrypto());
      } else if (pc.connectionState === 'disconnected') {
        if (!this.recovery) { this.recoveryStarted = performance.now(); this.recovery = setTimeout(() => this.close('Связь потеряна. Создайте новый чат.'), 10_000); }
        this.update({ phase: 'recovering' });
      } else if (pc.connectionState === 'failed' || pc.connectionState === 'closed') this.close('Соединение завершено. Создайте новый чат.');
    };
    pc.ondatachannel = event => this.attachChannel(event.channel);
    if (initiator) this.attachChannel(pc.createDataChannel('ssc-v4', { ordered: true }));
  }
  private attachChannel(channel: RTCDataChannel) {
    if (this.closed || this.channel || channel.label !== 'ssc-v4' || !channel.ordered || channel.maxRetransmits !== null || channel.maxPacketLifeTime !== null) {
      channel.close(); this.close('Недопустимый канал соединения.'); return;
    }
    this.channel = channel; channel.binaryType = 'arraybuffer'; channel.bufferedAmountLowThreshold = 64 * 1024;
    channel.onopen = () => {
      this.startHandshakeDeadline();
      void this.enqueue(async () => { await this.sendFrames(this.protocol!.start()); await this.updateCrypto(); });
    };
    channel.onmessage = event => {
      const size = event.data instanceof ArrayBuffer ? event.data.byteLength : event.data instanceof Blob ? event.data.size : Infinity;
      if (size > 64 * 1024 || this.incomingBytes + size > 8 * 1024 * 1024 || this.incomingFrames >= 512) { this.close('Входящая очередь переполнена. История очищена.'); return; }
      this.incomingBytes += size; this.incomingFrames++;
      void this.enqueue(async () => {
        try {
        const value: unknown = event.data;
        let frame: Uint8Array;
        if (value instanceof ArrayBuffer && value.byteLength <= 64 * 1024) frame = new Uint8Array(value);
        else if (value instanceof Blob && value.size <= 64 * 1024) frame = new Uint8Array(await value.arrayBuffer());
        else throw new Error('Invalid binary frame');
        this.ensure();
        const step = this.protocol!.receive(frame);
        // Authenticated Bye retires the core keys: handle it before any handshake checks
        // and before slower file/hash callbacks can delay history clearing.
        if (step.event?.type === 'bye') {
          this.close(step.event.reason === 'expired' ? 'Время чата истекло. История на этом устройстве очищена.' : 'Собеседник вышел. История чата на этом устройстве очищена.');
          return;
        }
        await this.sendFrames(step.outgoing);
        if (step.event) void this.conversation.receive(step.event);
        await this.updateCrypto();
        } finally { this.incomingBytes -= size; this.incomingFrames--; }
      });
    };
    channel.onclose = () => this.close('Собеседник отключился. Чат завершён.');
    channel.onerror = () => this.close('Ошибка защищённого канала. Создайте новый чат.');
  }
  private enqueue(operation: () => Promise<void>): Promise<void> {
    const next = this.sending.then(async () => { this.ensure(); await operation(); });
    this.sending = next.catch(() => { if (!this.closed) this.close('Не удалось безопасно продолжить соединение. Создайте новый чат.'); });
    return this.sending;
  }
  private startHandshakeDeadline() {
    if (this.protocol?.sas() || this.handshake) return;
    this.handshakeStarted = performance.now();
    this.handshake = setTimeout(() => this.close('Обмен ключами не завершился. Создайте новый чат.'), 20_000);
  }
  private async sendFrames(frames: Uint8Array[]) {
    for (const frame of frames) {
      this.ensure(); const channel = this.channel;
      if (!channel || channel.readyState !== 'open') throw new Error('Channel closed');
      await waitForSendBuffer(channel, this.controller.signal);
      this.ensure(); channel.send(frame as Uint8Array<ArrayBuffer>);
    }
  }
  private async updateCrypto() {
    this.ensure(); const protocol = this.protocol!; const sas = protocol.sas();
    if (!sas) return;
    clearTimeout(this.handshake); this.handshake = undefined;
    this.handshakeStarted = 0;
    const localConfirmed = protocol.local_ready(), peerConfirmed = protocol.peer_ready();
    this.update({ sas, localConfirmed, peerConfirmed });
    if (this.pc?.connectionState !== 'connected') return;
    if (localConfirmed && peerConfirmed) {
      if (!this.connectedPublished) {
        this.setMetadata(await this.api.connected(this.room!)); this.ensure(); this.connectedPublished = true;
      }
      this.update({ phase: 'chat' });
      if (!this.capabilitiesSent) { this.capabilitiesSent = true; void this.conversation.connected(playableVoiceFormats()); }
    } else this.update({ phase: localConfirmed ? 'waiting_peer' : 'verification' });
  }
  async confirm(): Promise<void> {
    if (this.closed || !this.protocol?.sas() || this.protocol.local_ready()) return;
    await this.enqueue(async () => {
      if (this.protocol!.local_ready()) return;
      await this.sendFrames([this.protocol!.confirm()]); await this.updateCrypto();
    });
  }
  async send(text: string): Promise<void> { await this.conversation.sendText(text); }
  private async sendEvent(event: ChatEvent) {
    if (this.closed || !this.protocol?.local_ready() || !this.protocol.peer_ready() || this.channel?.readyState !== 'open') throw new Error('Chat unavailable');
    await this.enqueue(async () => {
      const protocol = this.protocol!;
      const frames = event.type === 'text'
        ? protocol.send_text(event.text, Uint8Array.from(event.id.match(/../g)!, byte => parseInt(byte, 16)))
        : [protocol.send_event(JSON.stringify(event))];
      await this.sendFrames(frames);
    });
  }
  leave(keepalive = false) { this.close('Вы вышли. История чата на этом устройстве очищена.', keepalive, 'exit'); }
  private async localDescription(type: 'offer' | 'answer'): Promise<string> {
    const pc = this.pc!; const candidates: RTCIceCandidateInit[] = [];
    pc.onicecandidate = event => { if (event.candidate) candidates.push(event.candidate.toJSON()); };
    const description = type === 'offer' ? await pc.createOffer() : await pc.createAnswer();
    await pc.setLocalDescription(description);
    await new Promise<void>((resolve, reject) => {
      const changed = () => { if (pc.iceGatheringState === 'complete') { cleanup(); resolve(); } };
      const timer = setTimeout(() => { cleanup(); reject(new Error('ICE timeout')); }, 10_000);
      const cleanup = () => { clearTimeout(timer); pc.removeEventListener('icegatheringstatechange', changed); };
      pc.addEventListener('icegatheringstatechange', changed); changed();
    });
    this.ensure();
    if (!pc.localDescription?.sdp) throw new Error('Missing SDP');
    return this.module!.encode_invitation(this.id, type, pc.localDescription.sdp, JSON.stringify(candidates));
  }
  private async applyRemote(encoded: string, expected: 'offer' | 'answer') {
    const invitation = this.module!.decode_invitation(encoded); this.ensure();
    if (invitation.session_id !== this.id || invitation.sdp_type !== expected) throw new Error('Session mismatch');
    await this.pc!.setRemoteDescription({ type: expected, sdp: invitation.sdp });
    for (const candidate of invitation.ice_candidates) { this.ensure(); await this.pc!.addIceCandidate(candidate); }
    this.remoteApplied = true;
  }
  private async poll() {
    while (!this.closed && this.room) {
      try {
        if (this.initiator && !this.remoteApplied) {
          const answer = await this.api.getAnswer(this.room); this.ensure(); this.setMetadata(answer.room);
          if (answer.answer !== null) { await this.applyRemote(answer.answer, 'answer'); this.update({ phase: 'connecting' }); }
        } else this.setMetadata(await this.api.state(this.room));
      } catch (error) {
        if (this.closed) return;
        if (error instanceof RoomError && !['network', 'rate_limited', 'capacity'].includes(error.code)) { this.close(errorText(error)); return; }
      }
      await pause(2000);
    }
  }
  close(reason: string | null = null, keepalive = false, notifyPeer: 'exit' | 'expired' | false = false): void {
    if (this.closed) return;
    let graceful = false;
    if (notifyPeer && this.protocol?.local_ready() && this.protocol.peer_ready() && this.channel?.readyState === 'open') {
      try { this.channel.send(this.protocol.send_event(JSON.stringify({ type: 'bye', reason: notifyPeer })) as Uint8Array<ArrayBuffer>); graceful = true; } catch { /* Abrupt unload/network loss remains best effort. */ }
    }
    this.conversation.close(reason || 'Связь прервалась. История чата на этом устройстве очищена.');
    this.closed = true; this.controller.abort(); clearTimeout(this.recovery); clearTimeout(this.handshake); clearInterval(this.clock);
    const room = this.room; this.room = null; this.inviteSecret = ''; this.claimKey = ''; this.createKey = '';
    const channel = this.channel, pc = this.pc;
    if (channel) { channel.onclose = null; channel.onmessage = null; channel.onopen = null; channel.onerror = null; }
    if (pc) { pc.onconnectionstatechange = null; pc.ondatachannel = null; }
    this.channel = null; this.pc = null;
    const retire = () => { channel?.close(); pc?.close(); if (room) void this.api.end(room, keepalive); };
    if (graceful && !keepalive) setTimeout(retire, 250); else retire();
    if (this.protocol) {
      try { this.protocol.close(); } catch { /* Continue transport and capability teardown. */ }
      finally { try { this.protocol.free(); } catch { /* No state may be reused. */ } this.protocol = null; }
    }
    this.module = null; this.joinDeadline = null; this.sessionDeadline = null; this.joinAnchor = null; this.sessionAnchor = null; this.firstCreateAttempt = null; this.connectedAt = null; this.view = empty('closed'); this.update({ phase: 'closed', error: reason });
  }
}
