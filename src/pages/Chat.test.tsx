import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import Chat from './Chat';
const native = vi.hoisted(() => ({ invoke: vi.fn(), exit: vi.fn(), model: { sendText: vi.fn(), snapshot: vi.fn(), offerFiles: vi.fn(), accept: vi.fn(), reject: vi.fn(), cancel: vi.fn(), react: vi.fn(), forget: vi.fn() } }));
const state = { messages: [], closed: false, notice: null, cacheBytes: 0, peerAudioFormats: [] };
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('../chat/native', () => ({ useNativeChat: () => ({ model: native.model, state, capture: { platform: 'test', enabled: false, limitation: 'Unavailable in fixture' }, session: { generation: 1, phase: 'connected' }, error: null, remainingSeconds: 3558, exit: native.exit }) }));
beforeEach(() => { vi.clearAllMocks(); native.invoke.mockResolvedValue(null); native.exit.mockResolvedValue(undefined); native.model.sendText.mockResolvedValue(undefined); native.model.snapshot.mockReturnValue(state); });
it('uses the shared model for native text and exits through its adapter once', async () => {
  const back = vi.fn(); render(<Chat onBack={back} />);
  fireEvent.change(screen.getByLabelText('Сообщение'), { target: { value: 'native shared fixture' } });
  fireEvent.click(screen.getByRole('button', { name: 'Отправить' }));
  await waitFor(() => expect(native.model.sendText).toHaveBeenCalledWith('native shared fixture'));
  fireEvent.click(screen.getByRole('button', { name: 'Выйти из чата' }));
  expect(native.exit).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Выйти и очистить' }));
  await waitFor(() => expect(native.exit).toHaveBeenCalledOnce()); expect(back).toHaveBeenCalledOnce();
  expect(native.invoke).not.toHaveBeenCalledWith('send_text', expect.anything());
});
it('opens only a fixed native help source ID', async () => {
  render(<Chat onBack={vi.fn()} />); fireEvent.click(screen.getByRole('button', { name: 'Как защищён чат' }));
  fireEvent.click(screen.getByRole('link', { name: /RFC 7748/ }));
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('open_help_source', { source: 'x25519' }));
});
