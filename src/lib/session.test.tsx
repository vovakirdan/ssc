import { act, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { nativeListener, useSession } from './session';
const native = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: native.listen }));

describe('native subscriptions', () => {
  it('unregisters a listener resolving after unmount and ignores late events', async () => {
    let resolve: (unlisten: () => void) => void = () => {};
    const callback = vi.fn();
    let event: (payload: unknown) => void = () => {};
    native.listen.mockImplementation((_name, handler) => { event = handler; return new Promise(done => { resolve = done; }); });
    const stop = nativeListener('ssc-session', callback);
    stop();
    event({ payload: 'late' });
    const unlisten = vi.fn();
    resolve(unlisten);
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalledOnce();
    expect(callback).not.toHaveBeenCalled();
  });
  it('does not overwrite a live event with an older initialization snapshot', async () => {
    let resolve: (state: unknown) => void = () => {};
    let event: (payload: unknown) => void = () => {};
    native.invoke.mockReturnValue(new Promise(done => { resolve = done; }));
    native.listen.mockImplementation(async (name, handler) => { if (name === 'ssc-session') event = handler; return vi.fn(); });
    function State() { const { session } = useSession(); return <p>{session.phase}</p>; }
    render(<State />);
    act(() => event({ payload: { generation: 2, phase: 'connected', route: null } }));
    await act(async () => resolve({ generation: 1, phase: 'idle', route: null }));
    await waitFor(() => expect(screen.getByText('connected')).toBeInTheDocument());
  });
});
