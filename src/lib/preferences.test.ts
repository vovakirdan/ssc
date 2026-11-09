import { beforeEach, expect, it, vi } from 'vitest';
import { applySavedIceServers } from './preferences';
const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
beforeEach(() => { localStorage.clear(); native.invoke.mockReset().mockResolvedValue(true); });
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
it('new installations choose project mode and initialize access without receiving TURN credentials', async () => {
  await applySavedIceServers();
  expect(native.invoke.mock.calls).toEqual([['set_network_mode', { mode: 'project' }], ['initialize_access']]);
});
it('explicit disabled mode preserves its custom list without applying it', async () => {
  const servers = [{ id: 'saved', type: 'turn', url: 'turn:saved.example' }];
  localStorage.setItem('ssc-settings', JSON.stringify({ networkMode: 'disabled', servers }));
  await applySavedIceServers();
  expect(native.invoke).toHaveBeenCalledWith('set_network_mode', { mode: 'disabled' });
  expect(native.invoke).not.toHaveBeenCalledWith('set_ice_servers', expect.anything());
  expect(JSON.parse(localStorage.getItem('ssc-settings') || '{}').servers).toEqual(servers);
});
it('rejects invalid persisted mode instead of silently selecting project TURN', async () => {
  localStorage.setItem('ssc-settings', JSON.stringify({ networkMode: 'unknown', servers: [] }));
  await expect(applySavedIceServers()).rejects.toThrow('режим подключения повреждён');
  expect(native.invoke).not.toHaveBeenCalled();
});
it('custom routes become ready while unrelated activation is still pending', async () => {
  const servers = [{ id: 'own', type: 'stun', url: 'stun:own.example:3478' }];
  localStorage.setItem('ssc-settings', JSON.stringify({ servers }));
  native.invoke.mockImplementation((name: string) => name === 'initialize_access' ? new Promise(() => {}) : Promise.resolve(true));
  await applySavedIceServers();
  expect(native.invoke).toHaveBeenCalledWith('set_ice_servers', { servers });
});
