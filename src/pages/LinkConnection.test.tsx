import { StrictMode } from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import LinkConnection from './LinkConnection';
import type { LinkSnapshot } from '@/lib/link';

const native = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), copy: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: native.listen }));
let event: (value: { payload: LinkSnapshot }) => void;
const link = 'http://127.0.0.1:8080/i/opaque-test-token';
function snapshot(update: Partial<LinkSnapshot> = {}): LinkSnapshot {
  return { generation: 10, role: 'creator', phase: 'waiting_peer', url: link, expires_at: new Date(Date.now() + 300_000).toISOString(), error: null, ...update };
}
function emit(value: LinkSnapshot) { act(() => event({ payload: value })); }
function failure(retryable = true) { return { code: 'helper_network', stage: 'link', message: 'Помощник недоступен', retryable }; }
beforeEach(() => {
  native.invoke.mockReset().mockImplementation(async (command: string) => command === 'get_link_state' ? null : undefined);
  native.listen.mockReset().mockImplementation(async (_name: string, callback: typeof event) => { event = callback; return vi.fn(); });
  native.copy.mockReset().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: native.copy } });
});
it('starts only after an explicit click, creates with the configured TTL and copies the resulting URL', async () => {
  native.invoke.mockImplementation(async (command: string) => {
    if (command === 'get_link_state') return null;
    if (command === 'create_link_invitation') return snapshot({ phase: 'creating', url: null, expires_at: null });
  });
  render(<StrictMode><LinkConnection ttl={10} onBack={vi.fn()} /></StrictMode>);
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('get_link_state'));
  expect(native.invoke).not.toHaveBeenCalledWith('create_link_invitation', expect.anything());
  fireEvent.click(screen.getByRole('button', { name: 'Создать ссылку' }));
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('create_link_invitation', { ttlSeconds: 600 }));
  emit(snapshot());
  expect(screen.getByRole('textbox', { name: 'Короткая ссылка' })).toHaveValue(link);
  expect(screen.getByRole('status')).toHaveTextContent('Ожидаем собеседника');
  fireEvent.click(screen.getByRole('button', { name: 'Скопировать ссылку' }));
  await waitFor(() => expect(native.copy).toHaveBeenCalledWith(link));
});
it('preserves a pasted link on command rejection and sends it only after the join action', async () => {
  native.invoke.mockImplementation(async (command: string) => { if (command === 'get_link_state') return null; throw { message: 'Ссылка принадлежит другому сервису' }; });
  render(<LinkConnection ttl={5} onBack={vi.fn()} />);
  const input = screen.getByRole('textbox', { name: 'Ссылка приглашения' });
  fireEvent.change(input, { target: { value: ' https://other.invalid/i/test ' } });
  expect(native.invoke).not.toHaveBeenCalledWith('join_link_invitation', expect.anything());
  fireEvent.click(screen.getByRole('button', { name: 'Подключиться по ссылке' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('другому сервису');
  expect(input).toHaveValue(' https://other.invalid/i/test ');
  expect(native.invoke).toHaveBeenCalledWith('join_link_invitation', { url: 'https://other.invalid/i/test' });
});
it('retries the same guest generation rather than starting another answer operation', async () => {
  native.invoke.mockImplementation(async (command: string) => {
    if (command === 'get_link_state') return null;
    if (command === 'join_link_invitation') return snapshot({ role: 'guest', phase: 'sending_answer', url: null });
    if (command === 'retry_link_invitation') return snapshot({ role: 'guest', phase: 'sending_answer', url: null });
  });
  render(<LinkConnection ttl={5} onBack={vi.fn()} />);
  fireEvent.change(screen.getByRole('textbox', { name: 'Ссылка приглашения' }), { target: { value: link } });
  fireEvent.click(screen.getByRole('button', { name: 'Подключиться по ссылке' }));
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('join_link_invitation', { url: link }));
  emit(snapshot({ role: 'guest', phase: 'error', error: failure() }));
  expect(screen.getByRole('button', { name: 'Подключиться по ссылке' })).toBeDisabled();
  fireEvent.click(screen.getByRole('button', { name: 'Повторить подключение' }));
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('retry_link_invitation', { generation: 10 }));
  expect(native.invoke.mock.calls.filter(call => call[0] === 'join_link_invitation')).toHaveLength(1);
});
it('does not regress live state when an older command receipt or event arrives', async () => {
  let resolve: (state: LinkSnapshot) => void = () => {};
  native.invoke.mockImplementation((command: string) => command === 'create_link_invitation' ? new Promise(done => { resolve = done; }) : Promise.resolve(null));
  render(<LinkConnection ttl={5} onBack={vi.fn()} />);
  fireEvent.click(screen.getByRole('button', { name: 'Создать ссылку' }));
  emit(snapshot({ generation: 12 }));
  await act(async () => resolve(snapshot({ generation: 12, phase: 'creating', url: null })));
  emit(snapshot({ generation: 11, phase: 'error', error: failure() }));
  expect(screen.getByRole('textbox', { name: 'Короткая ссылка' })).toHaveValue(link);
  expect(screen.getByRole('status')).toHaveTextContent('Ожидаем собеседника');
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});
it('cancels the generation returned by an in-flight creation before leaving', async () => {
  let resolve: (state: LinkSnapshot) => void = () => {};
  native.invoke.mockImplementation((command: string) => command === 'create_link_invitation' ? new Promise(done => { resolve = done; }) : Promise.resolve(null));
  const onBack = vi.fn();
  render(<LinkConnection ttl={5} onBack={onBack} />);
  fireEvent.click(screen.getByRole('button', { name: 'Создать ссылку' }));
  fireEvent.click(screen.getByRole('button', { name: 'Назад' }));
  expect(onBack).not.toHaveBeenCalled();
  await act(async () => resolve(snapshot({ generation: 17, phase: 'creating', url: null })));
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('cancel_link_invitation', { generation: 17 }));
  expect(onBack).toHaveBeenCalledOnce();
});
it('unmounting for SAS verification does not cancel the Rust-owned operation', async () => {
  native.invoke.mockImplementation(async (command: string) => command === 'get_link_state' ? snapshot({ phase: 'waiting_connection' }) : undefined);
  const { unmount } = render(<LinkConnection ttl={5} onBack={vi.fn()} />);
  await screen.findByRole('textbox', { name: 'Короткая ссылка' });
  unmount();
  expect(native.invoke).not.toHaveBeenCalledWith('cancel_link_invitation', expect.anything());
});
it('ignores stale initialization and disposes listeners which resolve after unmount', async () => {
  let resolveSnapshot: (state: LinkSnapshot) => void = () => {};
  native.invoke.mockReturnValue(new Promise(done => { resolveSnapshot = done; }));
  const first = render(<LinkConnection ttl={5} onBack={vi.fn()} />);
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('get_link_state'));
  emit(snapshot({ generation: 20 }));
  await act(async () => resolveSnapshot(snapshot({ generation: 19, phase: 'error', error: failure() })));
  expect(screen.getByRole('textbox', { name: 'Короткая ссылка' })).toHaveValue(link);
  first.unmount();
  let resolveListener: (stop: () => void) => void = () => {};
  native.listen.mockReturnValue(new Promise(done => { resolveListener = done; }));
  const second = render(<LinkConnection ttl={5} onBack={vi.fn()} />);
  second.unmount(); const stop = vi.fn();
  await act(async () => resolveListener(stop));
  expect(stop).toHaveBeenCalledOnce();
});
it('accepts a newer initialization generation after an older event arrived', async () => {
  let resolveSnapshot: (state: LinkSnapshot) => void = () => {};
  native.invoke.mockReturnValue(new Promise(done => { resolveSnapshot = done; }));
  render(<LinkConnection ttl={5} onBack={vi.fn()} />);
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('get_link_state'));
  emit(snapshot({ generation: 21, phase: 'error', error: failure() }));
  await act(async () => resolveSnapshot(snapshot({ generation: 22, phase: 'complete', url: null })));
  expect(screen.getByRole('status')).toHaveTextContent('Соединение установлено');
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});
it('keeps live progress when initialization returns an older phase of the same generation', async () => {
  let resolveSnapshot: (state: LinkSnapshot) => void = () => {};
  native.invoke.mockReturnValue(new Promise(done => { resolveSnapshot = done; }));
  render(<LinkConnection ttl={5} onBack={vi.fn()} />);
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('get_link_state'));
  emit(snapshot({ generation: 23, phase: 'waiting_connection', url: null }));
  await act(async () => resolveSnapshot(snapshot({ generation: 23, phase: 'waiting_peer' })));
  expect(screen.getByRole('status')).toHaveTextContent('Устанавливаем соединение');
  expect(screen.queryByRole('textbox', { name: 'Короткая ссылка' })).not.toBeInTheDocument();
});
