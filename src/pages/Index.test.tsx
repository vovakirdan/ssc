import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import Index from './Index';
import type { SessionState, VerificationCode } from '@/lib/session';

type NativeEventHandler = (value: { payload: unknown }) => void;
const native = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), handlers: new Map<string, Set<NativeEventHandler>>() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: native.listen }));
vi.mock('./Welcome', () => ({ default: ({ onStart }: { onStart: () => void }) => <button onClick={onStart}>start</button> }));
vi.mock('./ConnectionHub', () => ({ default: ({ onLink }: { onLink: () => void }) => <button onClick={onLink}>links</button> }));
vi.mock('./GenerateQR', () => ({ default: () => null }));
vi.mock('./ScanQR', () => ({ default: () => null }));
vi.mock('./Settings', () => ({ default: () => null }));
vi.mock('./Chat', () => ({ default: () => null }));
vi.mock('@/components/DiagnosticsPanel', () => ({ default: () => null }));
function emit(name: string, payload: unknown) { act(() => native.handlers.get(name)?.forEach(callback => callback({ payload }))); }

beforeEach(() => {
  localStorage.clear();
  native.handlers.clear();
  native.invoke.mockReset();
  native.listen.mockReset().mockImplementation(async (name: string, callback: NativeEventHandler) => {
    const handlers = native.handlers.get(name) || new Set();
    handlers.add(callback); native.handlers.set(name, handlers);
    return () => { handlers.delete(callback); };
  });
});

it('ignores the cancelled link generation and approves only the replacement code actually displayed', async () => {
  let current: SessionState = { generation: 40, phase: 'idle', route: null };
  let fingerprint: VerificationCode | null = null;
  native.invoke.mockImplementation(async (name: string) => {
    if (name === 'get_link_state') return null;
    if (name === 'get_session_state') return current;
    if (name === 'get_fingerprint') return fingerprint;
    if (name === 'join_link_invitation') {
      current = { ...current, generation: current.generation + 1, phase: 'ice' };
      emit('ssc-session', current);
      return { generation: current.generation, role: 'guest', phase: 'waiting_connection', url: null, expires_at: null, error: null };
    }
    if (name === 'cancel_link_invitation') {
      current = { ...current, phase: 'closed' };
      emit('ssc-session', current);
    }
    return true;
  });
  render(<Index />);
  async function join(url: string) {
    const start = await screen.findByRole('button', { name: 'start' });
    await act(async () => {});
    fireEvent.click(start);
    fireEvent.click(await screen.findByRole('button', { name: 'links' }));
    fireEvent.change(await screen.findByRole('textbox', { name: 'Ссылка приглашения' }), { target: { value: url } });
    fireEvent.click(screen.getByRole('button', { name: 'Подключиться по ссылке' }));
    await screen.findByText('Устанавливаем соединение…');
  }
  await join('http://127.0.0.1:8080/i/A');
  fireEvent.click(screen.getByRole('button', { name: 'Назад' }));
  await join('http://127.0.0.1:8080/i/B');
  emit('ssc-sas', { generation: 41, sas: 'aaaa11112222' });
  expect(screen.queryByText('aaaa11112222')).not.toBeInTheDocument();
  expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
  expect(screen.getByText('Устанавливаем соединение…')).toBeInTheDocument();
  current = { ...current, phase: 'verification' };
  emit('ssc-session', current);
  fingerprint = { generation: 42, sas: 'bbbb33334444' };
  emit('ssc-sas', fingerprint);
  await screen.findByText(fingerprint.sas);
  emit('ssc-sas', { generation: 41, sas: 'aaaa11112222' });
  expect(screen.queryByText('aaaa11112222')).not.toBeInTheDocument();
  expect(screen.getByRole('checkbox')).not.toBeChecked();
  const confirm = screen.getByRole('button', { name: 'Коды совпадают — продолжить' });
  expect(confirm).toBeDisabled();
  fireEvent.click(screen.getByRole('checkbox'));
  fireEvent.click(confirm);
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('confirm_sas', fingerprint));
  expect(native.invoke.mock.calls.filter(call => call[0] === 'confirm_sas')).toHaveLength(1);
});
