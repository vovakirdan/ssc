import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionState } from '../lib/session';
const mock = vi.hoisted(() => ({
  session: { generation: 1, phase: 'connected', route: 'relay' } as SessionState,
  current: { current: { generation: 1, phase: 'connected', route: 'relay' } as SessionState },
  invoke: vi.fn(), listeners: new Map<string, Set<(event: { payload: SessionState }) => void>>(),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mock.invoke }));
vi.mock('../lib/session', () => ({
  useSession: () => ({ session: mock.session, current: mock.current, error: null }),
  nativeListener: (name: string, callback: (event: { payload: SessionState }) => void, _error: unknown, ready?: () => void) => {
    const callbacks = mock.listeners.get(name) || new Set(); callbacks.add(callback); mock.listeners.set(name, callbacks); ready?.(); return () => callbacks.delete(callback);
  },
}));
import { useNativeChat } from './native';
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
function snapshot(value: SessionState) { mock.session = value; mock.current.current = value; }
beforeEach(() => {
  mock.listeners.clear(); mock.invoke.mockReset(); snapshot({ generation: 1, phase: 'connected', route: 'relay' });
  mock.invoke.mockImplementation(async (command: string) => command === 'drain_chat_events' ? { generation: mock.session.generation, events: [] } : command === 'get_capture_protection' ? { platform: 'linux', enabled: false, limitation: 'unsupported' } : undefined);
});
describe('native shared chat generation boundary', () => {
  it('does not deliver a late drain from a replaced session', async () => {
    const old = deferred<{ generation: number; events: unknown[] }>();
    mock.invoke.mockImplementation((command: string, args?: { generation: number }) => command === 'drain_chat_events' && args?.generation === 1 ? old.promise : Promise.resolve(command === 'drain_chat_events' ? { generation: 2, events: [] } : undefined));
    const { result, rerender, unmount } = renderHook(() => useNativeChat());
    await waitFor(() => expect(mock.invoke).toHaveBeenCalledWith('drain_chat_events', { generation: 1 }));
    snapshot({ generation: 2, phase: 'connected', route: 'relay' }); rerender();
    await act(async () => old.resolve({ generation: 1, events: [{ type: 'text', id: 'a'.repeat(32), text: 'old secret' }] }));
    expect(result.current.state.messages).toEqual([]); expect(result.current.state.closed).toBe(false); unmount();
  });
  it('clears history on a peer-left snapshot without using a generic network error', async () => {
    let delivered = false;
    mock.invoke.mockImplementation(async (command: string) => command === 'drain_chat_events' ? { generation: 1, events: delivered ? [] : (delivered = true, [{ type: 'text', id: 'a'.repeat(32), text: 'secret' }]) } : undefined);
    const { result, unmount } = renderHook(() => useNativeChat());
    await waitFor(() => expect(result.current.state.messages).toHaveLength(1));
    await act(async () => { snapshot({ generation: 1, phase: 'closed', route: null, close_reason: 'peer_left' }); for (const callback of mock.listeners.get('ssc-session') || []) callback({ payload: mock.session }); });
    expect(result.current.state.messages).toEqual([]); expect(result.current.state.notice).toContain('Собеседник вышел'); unmount();
  });
  it('retires local history before the bounded native disconnect finishes', async () => {
    const pending = deferred<void>(); mock.invoke.mockImplementation(command => command === 'disconnect' ? pending.promise : Promise.resolve(command === 'drain_chat_events' ? { generation: 1, events: [] } : undefined));
    const { result, unmount } = renderHook(() => useNativeChat()); let exited!: Promise<void>;
    act(() => { exited = result.current.exit(); });
    expect(result.current.state.closed).toBe(true); expect(result.current.state.messages).toEqual([]);
    expect(mock.invoke).toHaveBeenCalledWith('disconnect', { reason: 'exit', generation: 1 });
    await act(async () => { pending.resolve(); await exited; }); unmount();
  });
  it('does not disconnect a replacement session through an old exit callback', async () => {
    const { result, unmount } = renderHook(() => useNativeChat()); const oldExit = result.current.exit;
    snapshot({ generation: 2, phase: 'connected', route: 'relay' });
    await act(async () => { await oldExit(); });
    expect(mock.invoke.mock.calls.some(([name]) => name === 'disconnect')).toBe(false); unmount();
  });
});
