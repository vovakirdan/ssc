import { invoke } from '@tauri-apps/api/core';

export type NetworkMode = 'project' | 'custom' | 'disabled';
export interface AccessStatus {
  state: 'active' | 'inactive' | 'error';
  access_reference?: string;
  error?: { code: string; stage: string; message: string; retryable: boolean };
}

export function savedNetworkMode(value: Record<string, unknown>): NetworkMode {
  if (value.networkMode === undefined) return 'custom'; // Existing lists always retain the user's choice.
  if (value.networkMode === 'project' || value.networkMode === 'custom' || value.networkMode === 'disabled') return value.networkMode;
  throw new Error('Сохранённый режим подключения повреждён. Проверьте настройки.');
}

// Apply mode before a session. Managed TURN secrets are obtained in Rust, never stored here.
export async function applySavedIceServers(): Promise<void> {
  const saved = localStorage.getItem('ssc-settings');
  if (!saved) {
    await invoke('set_network_mode', { mode: 'project' });
    await invoke('initialize_access');
    return;
  }
  let parsed: unknown;
  try { parsed = JSON.parse(saved); }
  catch { throw new Error('Сохранённые настройки серверов повреждены. Проверьте настройки.'); }
  if (typeof parsed !== 'object' || parsed === null || !('servers' in parsed) || !Array.isArray(parsed.servers)) {
    throw new Error('Сохранённые настройки серверов повреждены. Проверьте настройки.');
  }
  const mode = savedNetworkMode(parsed as Record<string, unknown>);
  if (mode === 'custom') await invoke('set_ice_servers', { servers: parsed.servers });
  else await invoke('set_network_mode', { mode });
  // A custom route must not wait for an unrelated activation service.
  if (mode === 'project') await invoke('initialize_access');
  else void invoke('initialize_access').catch(() => undefined);
}
