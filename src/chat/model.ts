import { AUTO_ACCEPT_BYTES, CHUNK_BYTES, MAX_CACHE_BYTES, MAX_FILE_BYTES, REACTIONS,
  type Attachment, type ChatEvent, type ChatItem, type ChatTransport, type ConversationSnapshot, type HashSink, type MediaKind, type ReactionEmoji } from './types';
export const randomId = () => Array.from(crypto.getRandomValues(new Uint8Array(16)), b => b.toString(16).padStart(2, '0')).join('');
const encode = (bytes: Uint8Array) => btoa(String.fromCharCode(...bytes));
function decode(value: string) { const data = atob(value); return Uint8Array.from(data, c => c.charCodeAt(0)); }
const kindOf = (file: File): MediaKind => file.type.startsWith('image/') && !file.type.includes('svg') ? 'photo' : file.type.startsWith('video/') ? 'video' : 'file';
interface Transfer { file?: Blob; parts: Blob[]; offset: number; acknowledged: number; hasher?: HashSink; cancelled: boolean; wake: Set<() => void>; }
/** Shared application policy. The transport owns cryptography; this object owns only volatile UI/file state. */
export class ChatModel {
  private state: ConversationSnapshot = { messages: [], closed: false, notice: null, cacheBytes: 0, peerAudioFormats: [] };
  private listeners = new Set<(state: ConversationSnapshot) => void>();
  private transfers = new Map<string, Transfer>();
  private pendingSends: string[] = [];
  private sending = false;
  private receiving: string | null = null;
  private receiveQueue: Promise<void> = Promise.resolve();
  private queuedEventBytes = 0;
  private queuedEvents = 0;
  private offerQueue: Promise<void> = Promise.resolve();
  private announced = false;
  constructor(private readonly transport: ChatTransport) {}
  snapshot = () => this.state;
  subscribe = (listener: (state: ConversationSnapshot) => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private publish() { this.state = { ...this.state, messages: [...this.state.messages] }; for (const listener of this.listeners) listener(this.state); }
  private notify(notice: string) { if (!this.state.closed) { this.state.notice = notice; this.publish(); } }
  private ensure() { if (this.state.closed || !this.transport.isReady()) throw new Error('chat_unavailable'); }
  private item(id: string) { return this.state.messages.find(message => message.id === id); }
  private live(id: string, transfer: Transfer) { return !this.state.closed && !transfer.cancelled && this.transfers.get(id) === transfer; }
  private add(item: ChatItem) {
    this.state.messages.push(item);
    // Text retention is independent of the explicitly managed attachment cache.
    const texts = this.state.messages.filter(message => message.text !== undefined);
    let bytes = texts.reduce((sum, message) => sum + new TextEncoder().encode(message.text || '').length, 0);
    while (texts.length > 128 || bytes > 8 * 1024 * 1024) { const oldest = texts.shift()!; bytes -= new TextEncoder().encode(oldest.text || '').length; this.state.messages = this.state.messages.filter(message => message !== oldest); }
    this.publish();
  }
  private base(id: string, own: boolean): ChatItem { return { id, own, status: own ? 'pending' : 'delivered', timestamp: Date.now(), reactions: { own: null, peer: null } }; }
  private async send(event: ChatEvent) { this.ensure(); await this.transport.send(event); }
  async connected(audio: string[] = ['audio/webm;codecs=opus', 'audio/mp4', 'audio/wav']) {
    if (this.announced || this.state.closed || !this.transport.isReady()) return;
    this.announced = true;
    try { await this.send({ type: 'capabilities', audio }); } catch { /* Connection lifecycle reports transport errors. */ }
  }
  async sendText(text: string) {
    if (!text.trim()) return;
    this.ensure(); if (new TextEncoder().encode(text).length > 256 * 1024) { this.notify('Сообщение превышает 256 KiB'); return; }
    const item = { ...this.base(randomId(), true), text }; this.add(item);
    try { await this.send({ type: 'text', id: item.id, text }); if (!this.state.closed && item.status === 'pending') item.status = 'sent'; }
    catch { if (!this.state.closed) { item.status = 'failed'; this.notify('Сообщение не доставлено. Проверьте соединение.'); } }
    if (!this.state.closed) this.publish();
  }
  async offerFiles(files: File[], hint?: MediaKind) {
    if (files.length > 4) { this.notify('Выберите не больше четырёх вложений за раз'); return; }
    this.offerQueue = this.offerQueue.then(() => this.offerBatch(files, hint));
    await this.offerQueue;
  }
  private async offerBatch(files: File[], hint?: MediaKind) {
    for (const file of files) {
      try {
        this.ensure();
        if (file.size > MAX_FILE_BYTES) { this.notify('Максимальный размер вложения — 100 MiB'); continue; }
        if (this.state.messages.filter(message => message.attachment).length >= 64 || this.state.messages.filter(message => message.own && message.attachment?.status === 'waiting').length >= 4) { this.notify('Сначала завершите или отмените ожидающие передачи'); continue; }
        if (!file.name || new TextEncoder().encode(file.name).length > 1024 || ['.', '..'].includes(file.name) || file.name.includes('/') || file.name.includes('\\') || Array.from(file.name).some(character => { const code = character.codePointAt(0)!; return code < 32 || (code >= 127 && code <= 159); })) { this.notify('Недопустимое имя файла: переименуйте файл перед отправкой'); continue; }
        const mime = file.type || 'application/octet-stream';
        if (mime.length > 128 || /[^\x20-\x7e]/.test(mime)) { this.notify('Неподдерживаемое описание типа файла'); continue; }
        if (this.state.cacheBytes + file.size > MAX_CACHE_BYTES) { this.notify('Временный буфер заполнен. Удалите ранее принятые вложения из памяти.'); continue; }
        const id = randomId(), transfer: Transfer = { file, parts: [], offset: 0, acknowledged: 0, cancelled: false, wake: new Set() };
        const attachment: Attachment = { id, name: file.name, mime, size: file.size, sha256: '', kind: hint || kindOf(file), status: 'waiting', progress: 0 };
        const item = { ...this.base(id, true), attachment }; this.transfers.set(id, transfer); this.state.cacheBytes += file.size; this.add(item);
        try {
          transfer.hasher = await this.transport.createHasher(id);
          if (transfer.cancelled || this.state.closed) { transfer.hasher.abort(); transfer.hasher = undefined; throw new Error('cancelled'); }
          for (let offset = 0; offset < file.size; offset += 256 * 1024) {
            if (transfer.cancelled || this.state.closed) throw new Error('cancelled');
            const bytes = new Uint8Array(await file.slice(offset, offset + 256 * 1024).arrayBuffer());
            try { await transfer.hasher.update(bytes); } finally { bytes.fill(0); }
          }
          attachment.sha256 = await transfer.hasher.finish(); transfer.hasher = undefined;
          if (transfer.cancelled || this.state.closed) throw new Error('cancelled');
          await this.send({ type: 'attachment_offer', id, name: attachment.name, mime: attachment.mime, size: file.size, sha256: attachment.sha256, kind: attachment.kind });
          if (this.live(id, transfer)) { item.status = 'sent'; this.publish(); }
        } catch { if (this.live(id, transfer)) { this.release(id, 'failed'); this.notify('Не удалось подготовить или передать вложение'); } }
      } catch { this.notify('Передача доступна после подтверждения кода с обеих сторон'); }
    }
  }
  receive(event: ChatEvent) {
    if (this.state.closed) return Promise.resolve();
    if (event.type === 'bye') { this.close(event.reason === 'expired' ? 'Время чата истекло. История на этом устройстве очищена.' : 'Собеседник вышел. История чата на этом устройстве очищена.'); this.transport.onPeerExit(event.reason); return Promise.resolve(); }
    const size = new TextEncoder().encode(JSON.stringify(event)).length;
    if (this.queuedEvents >= 512 || this.queuedEventBytes + size > 8 * 1024 * 1024) {
      this.close('Входящая очередь переполнена. История чата очищена.'); this.transport.onPeerExit('invalid'); return Promise.resolve();
    }
    this.queuedEvents++; this.queuedEventBytes += size;
    this.receiveQueue = this.receiveQueue.then(() => this.apply(event)).catch(() => { if (this.state.closed) return; this.close('Получено некорректное вложение. Чат очищен.'); this.transport.onPeerExit('invalid'); }).finally(() => { this.queuedEvents--; this.queuedEventBytes -= size; });
    return this.receiveQueue;
  }
  private async apply(event: ChatEvent) {
    if (this.state.closed) return;
    if (event.type === 'bye') { this.close(event.reason === 'expired' ? 'Время чата истекло. История на этом устройстве очищена.' : 'Собеседник вышел. История чата на этом устройстве очищена.'); this.transport.onPeerExit(event.reason); return; }
    if (event.type === 'capabilities') { this.state.peerAudioFormats = event.audio; this.publish(); return; }
    if (event.type === 'text') {
      if (!this.item(event.id)) this.add({ ...this.base(event.id, false), text: event.text });
      await this.send({ type: 'receipt', id: event.id }); return;
    }
    if (event.type === 'receipt') { const item = this.item(event.id); if (item?.own && item.text !== undefined) { item.status = 'delivered'; this.publish(); } return; }
    if (event.type === 'reaction') { const item = this.item(event.target); if (item) { item.reactions.peer = event.emoji; this.publish(); } return; }
    if (event.type === 'attachment_offer') {
      if (this.item(event.id)) return;
      if (this.state.messages.filter(message => message.attachment).length >= 64 || this.state.messages.filter(message => message.attachment?.status === 'offered').length >= 4) { await this.send({ type: 'attachment_reject', id: event.id }); return; }
      this.add({ ...this.base(event.id, false), attachment: { ...event, status: 'offered', progress: 0 } });
      if ((event.kind === 'photo' || event.kind === 'audio') && event.size <= AUTO_ACCEPT_BYTES && !this.receiving) await this.accept(event.id);
      return;
    }
    const item = this.item(event.id), attachment = item?.attachment;
    if (!item || !attachment) return;
    const transfer = this.transfers.get(event.id);
    switch (event.type) {
      case 'attachment_accept':
        if (!item.own || attachment.status !== 'waiting' || !transfer) return;
        attachment.status = 'sending'; this.pendingSends.push(event.id); this.publish(); void this.pump(); return;
      case 'attachment_reject': if (item.own) this.release(event.id, 'rejected'); return;
      case 'attachment_cancel': this.release(event.id, 'cancelled'); return;
      case 'attachment_progress':
        if (item.own && transfer && event.received >= transfer.acknowledged && event.received <= transfer.offset) { transfer.acknowledged = event.received; attachment.progress = event.received; for (const wake of transfer.wake) wake(); transfer.wake.clear(); this.publish(); }
        return;
      case 'attachment_complete':
        if (item.own && attachment.status === 'sending' && transfer && transfer.offset === attachment.size && transfer.acknowledged === attachment.size) { attachment.status = 'complete'; attachment.progress = attachment.size; attachment.file = transfer.file; item.status = 'delivered'; this.transfers.delete(event.id); this.makePreview(attachment); for (const wake of transfer.wake) wake(); transfer.wake.clear(); this.publish(); }
        return;
      case 'attachment_chunk':
        if (item.own || attachment.status !== 'receiving' || !transfer || !transfer.hasher) return;
        if (event.offset !== transfer.offset) { await this.cancel(event.id); return; }
        { const bytes = decode(event.data);
          if (bytes.length > CHUNK_BYTES || transfer.offset + bytes.length > attachment.size) { bytes.fill(0); await this.cancel(event.id); return; }
          try {
            await transfer.hasher.update(bytes);
            if (this.state.closed || transfer.cancelled || this.transfers.get(event.id) !== transfer) return;
            transfer.parts.push(new Blob([bytes as Uint8Array<ArrayBuffer>])); transfer.offset += bytes.length; attachment.progress = transfer.offset;
          } catch (error) {
            if (this.state.closed || transfer.cancelled || this.transfers.get(event.id) !== transfer) return;
            throw error;
          } finally { bytes.fill(0); }
          try { await this.send({ type: 'attachment_progress', id: event.id, received: transfer.offset }); }
          catch (error) { if (!this.live(event.id, transfer)) return; throw error; }
          if (!this.live(event.id, transfer)) return;
          if (transfer.offset === attachment.size) await this.finishReceiving(item, transfer);
          else this.publish();
        } return;
    }
  }
  async accept(id: string) {
    const item = this.item(id), attachment = item?.attachment;
    if (!attachment || item!.own || attachment.status !== 'offered') return;
    this.ensure();
    if (this.receiving) { this.notify('Дождитесь завершения текущего приёма'); return; }
    if (this.state.cacheBytes + attachment.size > MAX_CACHE_BYTES) { this.notify('Освободите временный буфер перед приёмом файла'); return; }
    const transfer: Transfer = { parts: [], offset: 0, acknowledged: 0, cancelled: false, wake: new Set() };
    this.transfers.set(id, transfer); this.receiving = id; attachment.status = 'receiving'; this.state.cacheBytes += attachment.size; this.publish();
    try {
      transfer.hasher = await this.transport.createHasher(id);
      if (this.state.closed || transfer.cancelled) { transfer.hasher.abort(); transfer.hasher = undefined; return; }
      await this.send({ type: 'attachment_accept', id });
      if (attachment.size === 0) await this.finishReceiving(item!, transfer);
    } catch { if (this.live(id, transfer)) { this.release(id, 'failed'); this.notify('Не удалось начать приём вложения'); } }
  }
  private async finishReceiving(item: ChatItem, transfer: Transfer) {
    const attachment = item.attachment!;
    if (!this.live(attachment.id, transfer) || !transfer.hasher) return;
    let hash: string;
    try { hash = await transfer.hasher.finish(); transfer.hasher = undefined; }
    catch (error) { if (!this.live(attachment.id, transfer)) return; throw error; }
    if (!this.live(attachment.id, transfer)) return;
    if (hash !== attachment.sha256) { await this.cancel(attachment.id); this.notify('Файл не прошёл проверку целостности'); return; }
    attachment.file = new Blob(transfer.parts, { type: attachment.mime }); transfer.parts = [];
    attachment.status = 'complete'; item.status = 'delivered'; this.receiving = null; this.transfers.delete(attachment.id); this.makePreview(attachment); this.publish();
    await this.send({ type: 'attachment_complete', id: attachment.id });
  }
  private async pump() {
    if (this.sending) return;
    this.sending = true;
    try {
      while (this.pendingSends.length && !this.state.closed) {
        const id = this.pendingSends.shift()!, transfer = this.transfers.get(id), attachment = this.item(id)?.attachment;
        if (!transfer?.file || !attachment || transfer.cancelled) continue;
        try {
          while (transfer.offset < attachment.size && !transfer.cancelled && !this.state.closed) {
            if (transfer.offset - transfer.acknowledged >= 256 * 1024) {
              await this.wait(transfer); continue;
            }
            const offset = transfer.offset, bytes = new Uint8Array(await transfer.file.slice(offset, offset + CHUNK_BYTES).arrayBuffer());
            try {
              if (transfer.cancelled || this.state.closed) break;
              transfer.offset += bytes.length;
              await this.send({ type: 'attachment_chunk', id, offset, data: encode(bytes) });
            } finally { bytes.fill(0); }
            // Yield between chunks so text/reactions and exit aren't starved.
            await new Promise(resolve => setTimeout(resolve, 0));
          }
          while (!transfer.cancelled && !this.state.closed && this.transfers.has(id)) await this.wait(transfer);
        } catch { this.release(id, 'failed'); this.notify('Передача прервана'); }
      }
    } finally { this.sending = false; }
  }
  private wait(transfer: Transfer) { return new Promise<void>(resolve => { const done = () => { clearTimeout(timer); transfer.wake.delete(done); resolve(); }; const timer = setTimeout(done, 500); transfer.wake.add(done); }); }
  async reject(id: string) { const item = this.item(id); if (item?.attachment && !item.own) { await this.send({ type: 'attachment_reject', id }); this.release(id, 'rejected'); } }
  async cancel(id: string) { this.release(id, 'cancelled'); if (!this.state.closed) try { await this.send({ type: 'attachment_cancel', id }); } catch { /* Lifecycle handles a disconnected transport. */ } }
  private release(id: string, status: Attachment['status']) {
    const attachment = this.item(id)?.attachment, transfer = this.transfers.get(id);
    if (!attachment) return;
    transfer?.hasher?.abort(); if (transfer) { transfer.cancelled = true; transfer.file = undefined; transfer.parts = []; for (const wake of transfer.wake) wake(); transfer.wake.clear(); }
    if (transfer || attachment.file) this.state.cacheBytes = Math.max(0, this.state.cacheBytes - attachment.size);
    this.transfers.delete(id); if (this.receiving === id) this.receiving = null;
    if (attachment.url) URL.revokeObjectURL(attachment.url);
    attachment.url = undefined; attachment.file = undefined; attachment.status = status; this.publish();
  }
  forget(id: string) {
    const item = this.item(id);
    if (!item) return;
    if (item.text !== undefined) { this.state.messages = this.state.messages.filter(message => message !== item); this.publish(); return; }
    const attachment = item.attachment;
    if (attachment && !['offered', 'sending', 'receiving', 'waiting'].includes(attachment.status)) this.release(id, 'forgotten');
  }
  async react(target: string, emoji: ReactionEmoji | null) { if (emoji !== null && !REACTIONS.includes(emoji)) return; const item = this.item(target); if (!item) return; await this.send({ type: 'reaction', target, emoji }); if (!this.state.closed) { item.reactions.own = emoji; this.publish(); } }
  private makePreview(attachment: Attachment) {
    if (!attachment.file || attachment.kind === 'file') return;
    // SVG/HTML and arbitrary documents are never embedded; unsupported codecs remain downloadable.
    if (!/^(image\/(jpeg|png|webp|gif|avif|heic|heif)|video\/(mp4|webm|quicktime)|audio\/(webm|mp4|wav|ogg))(;|$)/i.test(attachment.mime)) return;
    const file = attachment.file;
    void file.slice(0, 32).arrayBuffer().then(buffer => {
      if (this.state.closed || attachment.file !== file) return;
      const bytes = new Uint8Array(buffer), ascii = (start: number, length: number) => String.fromCharCode(...bytes.slice(start, start + length));
      const jpeg = bytes[0] === 255 && bytes[1] === 216 && bytes[2] === 255;
      const png = bytes[0] === 137 && ascii(1, 3) === 'PNG';
      const gif = ascii(0, 3) === 'GIF', riff = ascii(0, 4) === 'RIFF';
      const webm = bytes[0] === 26 && bytes[1] === 69 && bytes[2] === 223 && bytes[3] === 163;
      const iso = ascii(4, 4) === 'ftyp';
      const safe = attachment.kind === 'photo' ? jpeg || png || gif || (riff && ascii(8, 4) === 'WEBP') || iso
        : attachment.kind === 'audio' ? (riff && ascii(8, 4) === 'WAVE') || webm || iso || ascii(0, 4) === 'OggS'
          : webm || iso;
      if (safe) { attachment.url = URL.createObjectURL(file); this.publish(); }
    }).catch(() => { /* Unsupported preview never invalidates the original file. */ });
  }
  close(reason = 'Чат завершён. История на этом устройстве очищена.') {
    if (this.state.closed) return;
    for (const transfer of this.transfers.values()) { transfer.cancelled = true; transfer.hasher?.abort(); transfer.file = undefined; transfer.parts = []; for (const wake of transfer.wake) wake(); transfer.wake.clear(); }
    for (const item of this.state.messages) if (item.attachment) { if (item.attachment.url) URL.revokeObjectURL(item.attachment.url); item.attachment.file = undefined; item.attachment.url = undefined; }
    this.transfers.clear(); this.pendingSends = []; this.receiving = null;
    this.state = { messages: [], closed: true, notice: reason, cacheBytes: 0, peerAudioFormats: [] }; this.publish();
  }
}
