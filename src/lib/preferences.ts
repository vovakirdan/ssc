import { invoke } from '@tauri-apps/api/core';

// Apply before creating a session so a saved custom TURN never silently falls back.
export async function applySavedIceServers(): Promise<void> {
  const saved = localStorage.getItem('ssc-settings');
  if (!saved) return;
  let parsed: unknown;
  try { parsed = JSON.parse(saved); }
  catch { throw new Error('Сохранённые настройки серверов повреждены. Проверьте настройки.'); }
  if (typeof parsed !== 'object' || parsed === null || !('servers' in parsed) || !Array.isArray(parsed.servers)) {
    throw new Error('Сохранённые настройки серверов повреждены. Проверьте настройки.');
  }
  await invoke('set_ice_servers', { servers: parsed.servers });
}
