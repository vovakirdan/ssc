import { beforeEach, expect, it, vi } from 'vitest';
import { applySavedIceServers } from './preferences';
const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
beforeEach(() => { localStorage.clear(); native.invoke.mockReset(); });
it('hydrates custom TURN before sessions are allowed', async () => {
  const servers = [{ id: 'own', type: 'turn', url: 'turn:example.test:3478', username: 'user', credential: 'secret' }];
  localStorage.setItem('ssc-settings', JSON.stringify({ servers }));
  await applySavedIceServers();
  expect(native.invoke).toHaveBeenCalledWith('set_ice_servers', { servers });
});
it('does not silently fall back when saved servers fail validation', async () => {
  localStorage.setItem('ssc-settings', JSON.stringify({ servers: [{ url: 'invalid' }] }));
  native.invoke.mockRejectedValue({ message: 'Некорректный сервер' });
  await expect(applySavedIceServers()).rejects.toEqual({ message: 'Некорректный сервер' });
});
it('reports malformed stored JSON without echoing it', async () => {
  localStorage.setItem('ssc-settings', '{"credential":"secret');
  await expect(applySavedIceServers()).rejects.toThrow('Сохранённые настройки серверов повреждены');
  expect(native.invoke).not.toHaveBeenCalled();
});
