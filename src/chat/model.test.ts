import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { Blob as NodeBlob, File as NodeFile } from 'node:buffer';
import { ChatModel } from './model';
import { MAX_CACHE_BYTES, MAX_FILE_BYTES, type ChatEvent, type HashSink } from './types';
const hasher = (): HashSink => { const value = createHash('sha256'); return { update: async bytes => { value.update(bytes); }, finish: async () => value.digest('hex'), abort: vi.fn() }; };
const file = (bytes: Uint8Array | string, name = 'original.bin', type = 'application/octet-stream') => new NodeFile([typeof bytes === 'string' ? bytes : Buffer.from(bytes)], name, { type }) as unknown as File;
function pair() {
  const events: ChatEvent[] = [], exits: string[] = [];
  const a: ChatModel = new ChatModel({ isReady: () => true, createHasher: async () => hasher(), onPeerExit: reason => exits.push(reason), send: async event => { events.push(event); queueMicrotask(() => { void b.receive(structuredClone(event)); }); } });
  const b: ChatModel = new ChatModel({ isReady: () => true, createHasher: async () => hasher(), onPeerExit: reason => exits.push(reason), send: async event => { events.push(event); queueMicrotask(() => { void a.receive(structuredClone(event)); }); } });
  return { a, b, events, exits };
}
async function until(test: () => boolean) { await vi.waitFor(() => expect(test()).toBe(true), { timeout: 8000, interval: 10 }); }
beforeEach(() => { vi.stubGlobal('Blob', NodeBlob); Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: vi.fn(() => 'blob:owned-test') }); Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: vi.fn() }); });
afterEach(() => vi.unstubAllGlobals());
describe('shared encrypted chat application policy', () => {
  it('uses common text IDs for receipts and replaceable reactions', async () => {
    const { a, b } = pair(); await a.sendText('Привет 👋');
    await until(() => a.snapshot().messages[0]?.status === 'delivered');
    const id = a.snapshot().messages[0].id; expect(b.snapshot().messages[0]).toMatchObject({ id, text: 'Привет 👋', own: false });
    await b.react(id, '❤️'); await until(() => a.snapshot().messages[0].reactions.peer === '❤️');
    await b.react(id, null); await until(() => a.snapshot().messages[0].reactions.peer === null); a.close(); b.close();
  });
  it('waits for explicit acceptance, verifies original bytes and allows text during chunks', async () => {
    const { a, b, events } = pair(); const bytes = new Uint8Array(512 * 1024).map((_, i) => i % 251);
    await a.offerFiles([file(bytes, 'metadata-original.bin')]); await until(() => b.snapshot().messages.length === 1);
    const id = b.snapshot().messages[0].id; expect(events.some(event => event.type === 'attachment_chunk')).toBe(false);
    await b.accept(id); await a.sendText('While file transfers');
    await until(() => b.snapshot().messages[0]?.attachment?.status === 'complete');
    const attachment = b.snapshot().messages[0].attachment!;
    expect(new Uint8Array(await attachment.file!.arrayBuffer())).toEqual(bytes); expect(attachment.name).toBe('metadata-original.bin');
    expect(b.snapshot().messages.some(message => message.text === 'While file transfers')).toBe(true);
    await until(() => a.snapshot().messages[0]?.status === 'delivered'); a.close(); b.close();
  });
  it('autoaccepts small photos/voice but never arbitrary files or videos', async () => {
    const { a, b } = pair(); await a.offerFiles([file('audio', 'voice.wav', 'audio/wav')], 'audio');
    await until(() => b.snapshot().messages[0]?.attachment?.status === 'complete');
    await a.offerFiles([file('video', 'clip.mp4', 'video/mp4')]); await until(() => b.snapshot().messages.length === 2);
    expect(b.snapshot().messages[1].attachment?.status).toBe('offered'); a.close(); b.close();
  });
  it('does not reserve twice when accept is pressed concurrently', async () => {
    const { a, b } = pair(); await a.offerFiles([file('bytes')]); await until(() => b.snapshot().messages.length === 1);
    const id = b.snapshot().messages[0].id; await Promise.all([b.accept(id), b.accept(id)]);
    await until(() => b.snapshot().messages[0].attachment?.status === 'complete'); expect(b.snapshot().cacheBytes).toBe(5); a.close(); b.close();
  });
  it('rejects above100MiB before reading file content', async () => {
    const { a, b, events } = pair(); const source = file('tiny'); Object.defineProperty(source, 'size', { value: MAX_FILE_BYTES + 1 });
    await a.offerFiles([source]); expect(events).toHaveLength(0); expect(a.snapshot().notice).toContain('100 MiB'); a.close(); b.close();
  });
  it('rejects incorrect final hash and discards partial bytes', async () => {
    const { a, b } = pair(); const id = '1'.repeat(32);
    await b.receive({ type: 'attachment_offer', id, name: 'bad.bin', mime: 'application/octet-stream', size: 3, sha256: '0'.repeat(64), kind: 'file' });
    await b.accept(id); await b.receive({ type: 'attachment_chunk', id, offset: 0, data: btoa('bad') });
    expect(b.snapshot().messages[0].attachment?.status).toBe('cancelled'); expect(b.snapshot().cacheBytes).toBe(0); expect(b.snapshot().messages[0].attachment?.file).toBeUndefined(); a.close(); b.close();
  });
  it('clears buffers, messages and previews immediately on authenticated exit', async () => {
    const { a, b, exits } = pair(); await a.sendText('sensitive'); await until(() => b.snapshot().messages.length === 1);
    await b.receive({ type: 'bye', reason: 'exit' }); expect(b.snapshot()).toMatchObject({ closed: true, messages: [], cacheBytes: 0 }); expect(exits).toContain('exit'); a.close();
  });
  it('ignores a late receive failure after close and wipes its in-flight chunk', async () => {
    let reject!: (error: Error) => void;
    const stalled = new Promise<void>((_resolve, fail) => { reject = fail; });
    let captured: Uint8Array | undefined;
    const peerExit = vi.fn();
    const model = new ChatModel({ isReady: () => true, send: async () => {}, onPeerExit: peerExit,
      createHasher: async () => ({ update: bytes => { captured = bytes; return stalled; }, finish: async () => '0'.repeat(64), abort: vi.fn() }) });
    const id = 'a'.repeat(32);
    await model.receive({ type: 'attachment_offer', id, name: 'owned.bin', mime: 'application/octet-stream', size: 3, sha256: '0'.repeat(64), kind: 'file' });
    await model.accept(id);
    const receiving = model.receive({ type: 'attachment_chunk', id, offset: 0, data: btoa('abc') });
    await until(() => !!captured); model.close(); reject(new Error('old hash cancelled')); await receiving;
    expect(peerExit).not.toHaveBeenCalled(); expect(captured?.every(byte => byte === 0)).toBe(true);
    expect(model.snapshot()).toMatchObject({ closed: true, messages: [], cacheBytes: 0 });
  });
  it('accepts empty files and permits explicit forgetting without storing history', async () => {
    const { a, b } = pair(); await a.offerFiles([file('')]); await until(() => b.snapshot().messages.length === 1);
    const id = b.snapshot().messages[0].id; await b.accept(id); await until(() => b.snapshot().messages[0].attachment?.status === 'complete');
    b.forget(id); expect(b.snapshot().messages[0].attachment?.status).toBe('forgotten'); expect(b.snapshot().cacheBytes).toBe(0); a.close(); b.close();
  });
  it('reserves200MiB without silently evicting original media', async () => {
    const { a, b, events } = pair();
    const source = file('x'); Object.defineProperty(source, 'size', { value: MAX_FILE_BYTES });
    vi.spyOn(source, 'slice').mockImplementation(() => new NodeBlob([]) as unknown as Blob);
    await a.offerFiles([source, source]);
    expect(a.snapshot().cacheBytes).toBe(MAX_CACHE_BYTES); expect(a.snapshot().messages).toHaveLength(2);
    const offers = events.filter(event => event.type === 'attachment_offer').length;
    await a.offerFiles([file('third')]); expect(events.filter(event => event.type === 'attachment_offer')).toHaveLength(offers);
    expect(a.snapshot().notice).toContain('буфер'); expect(a.snapshot().messages).toHaveLength(2);
    a.close(); b.close(); expect(a.snapshot().cacheBytes).toBe(0);
  });
  it('does not mark an unaccepted attachment delivered through a receipt or zero-byte completion', async () => {
    const { a, b } = pair(); await a.offerFiles([file('')]); const item = a.snapshot().messages[0];
    await a.receive({ type: 'receipt', id: item.id }); await a.receive({ type: 'attachment_complete', id: item.id });
    expect(item.attachment?.status).toBe('waiting'); expect(item.status).not.toBe('delivered'); a.close(); b.close();
  });
  it('rejects a local invalid filename/MIME without ending a verified chat', async () => {
    const { a, b, events } = pair();
    await a.offerFiles([file('x', 'bad\\name')]); const wrongType = file('x'); Object.defineProperty(wrongType, 'type', { value: 'a'.repeat(129) });
    await a.offerFiles([wrongType]); expect(events.filter(event => event.type === 'attachment_offer')).toHaveLength(0);
    expect(a.snapshot().cacheBytes).toBe(0); expect(a.snapshot().closed).toBe(false);
    await a.sendText('still works'); await until(() => b.snapshot().messages.some(item => item.text === 'still works')); a.close(); b.close();
  });
  it('bounds queued application events even while a hasher stalls', async () => {
    let release!: () => void; const gate = new Promise<void>(resolve => { release = resolve; }); let entered = false; const peerExit = vi.fn();
    const model = new ChatModel({ isReady: () => true, send: async () => {}, onPeerExit: peerExit, createHasher: async () => ({ update: async () => { entered = true; await gate; }, finish: async () => '0'.repeat(64), abort: vi.fn() }) });
    const id = 'a'.repeat(32), data = btoa('x'.repeat(16 * 1024));
    await model.receive({ type: 'attachment_offer', id, name: 'large.bin', mime: 'application/octet-stream', size: 100 * 1024 * 1024, sha256: '0'.repeat(64), kind: 'file' }); await model.accept(id);
    const first = model.receive({ type: 'attachment_chunk', id, offset: 0, data }); await until(() => entered);
    const queued = Array.from({ length: 520 }, (_, i) => model.receive({ type: 'attachment_chunk', id, offset: (i + 1) * 16 * 1024, data }));
    expect(model.snapshot().closed).toBe(true); expect(model.snapshot().cacheBytes).toBe(0); expect(peerExit).toHaveBeenCalledOnce();
    release(); await first; await Promise.all(queued);
  });
  it('prioritizes authenticated Bye over a stalled file callback', async () => {
    let release!: () => void; const gate = new Promise<void>(resolve => { release = resolve; }); let entered = false; const peerExit = vi.fn();
    const model = new ChatModel({ isReady: () => true, send: async () => {}, onPeerExit: peerExit, createHasher: async () => ({ update: async () => { entered = true; await gate; }, finish: async () => '0'.repeat(64), abort: vi.fn() }) });
    const id = 'a'.repeat(32);
    await model.receive({ type: 'attachment_offer', id, name: 'x.bin', mime: 'application/octet-stream', size: 3, sha256: '0'.repeat(64), kind: 'file' }); await model.accept(id);
    const chunk = model.receive({ type: 'attachment_chunk', id, offset: 0, data: btoa('abc') }); await until(() => entered);
    await model.receive({ type: 'bye', reason: 'exit' }); expect(model.snapshot().messages).toEqual([]); expect(model.snapshot().notice).toContain('Собеседник вышел');
    release(); await chunk; expect(peerExit).toHaveBeenCalledExactlyOnceWith('exit');
  });
  it.each(['progress', 'finish'])('cancel during a pending %s operation does not close the chat', async boundary => {
    let fail!: (error: Error) => void; const gate = new Promise<void>((_resolve, reject) => { fail = reject; }); let entered = false; const peerExit = vi.fn();
    const model = new ChatModel({ isReady: () => true, onPeerExit: peerExit,
      send: async event => { if (event.type === 'attachment_progress' && boundary === 'progress') { entered = true; await gate; } },
      createHasher: async () => ({ update: async () => {}, finish: async () => { entered = true; await gate; return '0'.repeat(64); }, abort: vi.fn() }) });
    const id = 'a'.repeat(32);
    await model.receive({ type: 'attachment_offer', id, name: 'x.bin', mime: 'application/octet-stream', size: 3, sha256: '0'.repeat(64), kind: 'file' }); await model.accept(id);
    const chunk = model.receive({ type: 'attachment_chunk', id, offset: 0, data: btoa('abc') }); await until(() => entered);
    await model.cancel(id); fail(new Error('cancelled operation')); await chunk;
    expect(model.snapshot().closed).toBe(false); expect(model.snapshot().cacheBytes).toBe(0); expect(model.snapshot().messages[0].attachment?.status).toBe('cancelled'); expect(peerExit).not.toHaveBeenCalled(); model.close();
  });

});
