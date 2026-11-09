export type RoomState = 'waiting' | 'joined' | 'answered' | 'connected';
export interface RoomMetadata { id: string; state: RoomState; join_expires_at: string; expires_at: string | null }
export interface CreatedRoom extends RoomMetadata { creator_token: string; invitation_url: string }
export interface JoinedRoom extends RoomMetadata { guest_token: string; offer: string }
export interface RoomAccess { id: string; token: string }
export class RoomError extends Error {
  constructor(readonly code: string) { super('Room operation failed'); }
}
export const isCapability = (value: unknown): value is string => typeof value === 'string' && /^[A-Za-z0-9_-]{43}$/.test(value);
const object = (value: unknown): Record<string, unknown> => {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new RoomError('invalid_response');
  return value as Record<string, unknown>;
};
const date = (value: unknown): value is string => typeof value === 'string' && value.length <= 40 && Number.isFinite(Date.parse(value));
export function metadata(value: unknown): RoomMetadata {
  const data = object(value);
  if (!isCapability(data.id) || !['waiting', 'joined', 'answered', 'connected'].includes(String(data.state)) || !date(data.join_expires_at) || !(data.expires_at === null || date(data.expires_at))) throw new RoomError('invalid_response');
  if ((data.state === 'connected') !== (data.expires_at !== null)) throw new RoomError('invalid_response');
  return { id: data.id, state: data.state as RoomState, join_expires_at: data.join_expires_at, expires_at: data.expires_at as string | null };
}
const projectTurnUrls = new Set([
  'turn:turn.vovakirdan.ru:3478?transport=udp',
  'turn:turn.vovakirdan.ru:3478?transport=tcp',
  'turns:turn.vovakirdan.ru:5349?transport=tcp',
]);
const fixtureTurnUrls = new Set([
  'turn:127.0.0.1:3478?transport=udp',
  'turn:127.0.0.1:3478?transport=tcp',
  'turns:127.0.0.1:5349?transport=tcp',
]);
function allowedTurnUrl(value: unknown): value is string {
  if (typeof value !== 'string') return false;
  if (projectTurnUrls.has(value)) return true;
  return ['127.0.0.1', 'localhost', '[::1]'].includes(location.hostname) && fixtureTurnUrls.has(value);
}
const errors = new Set(['room_unavailable', 'conflict', 'offer_pending', 'not_ready', 'rate_limited', 'capacity']);
function temporaryStatus(status: number): string | null {
  return status === 429 ? 'rate_limited' : status === 503 ? 'capacity' : status === 502 || status === 504 ? 'network' : null;
}
export class RoomsApi {
  constructor(private readonly signal: AbortSignal) {}
  private async request(path: string, method: string, body?: unknown, token?: string, idempotency?: string): Promise<unknown> {
    const timeout = new AbortController();
    const abort = () => timeout.abort();
    this.signal.addEventListener('abort', abort, { once: true });
    if (this.signal.aborted) timeout.abort();
    const timer = setTimeout(abort, 10_000);
    try {
      const response = await fetch(`/v1/rooms${path}`, {
        method, credentials: 'omit', cache: 'no-store', redirect: 'error', referrerPolicy: 'no-referrer', signal: timeout.signal,
        headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}), ...(idempotency ? { 'Idempotency-Key': idempotency } : {}) },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
      // Proxies can return HTML for temporary failures. HTTP status remains authoritative.
      const retryCode = temporaryStatus(response.status);
      if (retryCode) {
        void response.body?.cancel().catch(() => undefined);
        throw new RoomError(retryCode);
      }
      const reader = response.body?.getReader();
      if (!reader) throw new RoomError('invalid_response');
      const chunks: Uint8Array[] = []; let size = 0;
      while (true) {
        const { done, value } = await reader.read(); if (done) break;
        size += value.byteLength;
        if (size > 1024 * 1024) { await reader.cancel(); throw new RoomError('invalid_response'); }
        chunks.push(value);
      }
      const bytes = new Uint8Array(size); let offset = 0;
      for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
      let decoded: unknown;
      try { decoded = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)); } catch { throw new RoomError('invalid_response'); }
      if (!response.ok) {
        const code = object(decoded).error;
        throw new RoomError(typeof code === 'string' && errors.has(code) ? code : 'request_failed');
      }
      return decoded;
    } catch (failure) {
      if (failure instanceof RoomError) throw failure;
      throw new RoomError('network');
    } finally { clearTimeout(timer); this.signal.removeEventListener('abort', abort); }
  }
  async create(key: string): Promise<CreatedRoom> {
    const data = object(await this.request('', 'POST', {}, undefined, key)); const room = metadata(data);
    if (!isCapability(data.creator_token) || typeof data.invitation_url !== 'string') throw new RoomError('invalid_response');
    const url = new URL(data.invitation_url, location.origin);
    if (url.username || url.password || url.origin !== location.origin || url.pathname !== `/r/${room.id}` || !/^#k=[A-Za-z0-9_-]{43}$/.test(url.hash) || url.search) throw new RoomError('invalid_response');
    return { ...room, creator_token: data.creator_token, invitation_url: url.href };
  }
  async join(id: string, inviteSecret: string, claimKey: string): Promise<JoinedRoom> {
    const data = object(await this.request(`/${id}/join`, 'POST', { invite_secret: inviteSecret, claim_key: claimKey }));
    if (!isCapability(data.guest_token) || typeof data.offer !== 'string' || data.offer.length > 512 * 1024) throw new RoomError('invalid_response');
    return { ...metadata(data), guest_token: data.guest_token, offer: data.offer };
  }
  async offer(room: RoomAccess, offer: string): Promise<RoomMetadata> { return metadata(await this.request(`/${room.id}/offer`, 'POST', { offer }, room.token)); }
  async answer(room: RoomAccess, answer: string): Promise<RoomMetadata> { return metadata(await this.request(`/${room.id}/answer`, 'POST', { answer }, room.token)); }
  async getAnswer(room: RoomAccess): Promise<{ answer: string | null; room: RoomMetadata }> {
    const data = object(await this.request(`/${room.id}/answer`, 'GET', undefined, room.token));
    if (!(data.answer === null || (typeof data.answer === 'string' && data.answer.length <= 512 * 1024))) throw new RoomError('invalid_response');
    return { answer: data.answer as string | null, room: metadata(data) };
  }
  async state(room: RoomAccess): Promise<RoomMetadata> { return metadata(await this.request(`/${room.id}`, 'GET', undefined, room.token)); }
  async connected(room: RoomAccess): Promise<RoomMetadata> { return metadata(await this.request(`/${room.id}/connected`, 'POST', {}, room.token)); }
  async turn(room: RoomAccess): Promise<RTCIceServer[]> {
    const data = object(await this.request(`/${room.id}/turn`, 'POST', {}, room.token));
    if (!Array.isArray(data.ice_servers) || data.ice_servers.length > 8 || !date(data.expires_at)) throw new RoomError('invalid_response');
    return data.ice_servers.map(raw => {
      const value = object(raw);
      if (!Array.isArray(value.urls) || value.urls.length < 1 || value.urls.length > 3 || !value.urls.every(allowedTurnUrl) || typeof value.username !== 'string' || value.username.length > 256 || typeof value.credential !== 'string' || value.credential.length > 256) throw new RoomError('invalid_response');
      return { urls: value.urls as string[], username: value.username, credential: value.credential };
    });
  }
  async end(room: RoomAccess, keepalive = false): Promise<void> {
    // Cleanup survives the already-aborted session signal; the UI retires immediately.
    const path = `/v1/rooms/${room.id}`, authorization = `Bearer ${room.token}`;
    const controller = new AbortController();
    const deadline = performance.now() + 5000;
    const timer = keepalive ? undefined : setTimeout(() => controller.abort(), 5000);
    try {
      for (let attempt = 0; attempt < (keepalive ? 1 : 3); attempt++) {
        if (!keepalive && (controller.signal.aborted || performance.now() >= deadline)) return;
        try {
          const response = await fetch(path, {
            method: 'DELETE', headers: { Authorization: authorization }, credentials: 'omit', cache: 'no-store', redirect: 'error', referrerPolicy: 'no-referrer', keepalive,
            ...(keepalive ? {} : { signal: controller.signal }),
          });
          void response.body?.cancel().catch(() => undefined);
          // Includes success, already-closed404 and permanent client errors.
          if (!temporaryStatus(response.status)) return;
        } catch { if (controller.signal.aborted) return; }
        if (keepalive || attempt === 2) return;
        const remaining = deadline - performance.now();
        if (remaining <= 0) return;
        await new Promise<void>(resolve => {
          const done = () => { clearTimeout(delay); controller.signal.removeEventListener('abort', done); resolve(); };
          const delay = setTimeout(done, Math.min(attempt === 0 ? 500 : 1000, remaining));
          controller.signal.addEventListener('abort', done, { once: true });
          if (controller.signal.aborted) done();
        });
      }
    } finally { clearTimeout(timer); }
  }
}
