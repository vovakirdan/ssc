import { afterEach, expect, it, vi } from 'vitest';
import { ChatModel } from './model';
import type { ChatTransport } from './types';

afterEach(() => vi.unstubAllGlobals());
it('draws one local alias per model, keeps it through updates, and never sends or persists it', async () => {
  let draws = 0;
  const random = vi.fn((bytes: Uint8Array) => { bytes.fill(draws++ === 0 ? 0 : 255); return bytes; });
  vi.stubGlobal('crypto', { getRandomValues: random });
  const write = vi.spyOn(Storage.prototype, 'setItem'), send = vi.fn().mockResolvedValue(undefined);
  const transport: ChatTransport = { isReady: () => true, send, onPeerExit: vi.fn(), createHasher: vi.fn() };
  const first = new ChatModel(transport);
  expect(first.peerName).toBe('Тихий Енот'); expect(random).toHaveBeenCalledTimes(1);
  const second = new ChatModel(transport);
  expect(second.peerName).toBe('Пушистый Пингвин'); expect(random).toHaveBeenCalledTimes(2);
  await first.receive({ type: 'text', id: 'a'.repeat(32), text: 'owned fixture' }); await first.sendText('reply');
  expect(first.peerName).toBe('Тихий Енот'); expect(second.peerName).toBe('Пушистый Пингвин');
  expect(first.snapshot()).not.toHaveProperty('peerName'); expect(write).not.toHaveBeenCalled();
  expect(JSON.stringify(send.mock.calls)).not.toContain(first.peerName); expect(JSON.stringify(send.mock.calls)).not.toContain(second.peerName);
  first.close(); second.close();
});
