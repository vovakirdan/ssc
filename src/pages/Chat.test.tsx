import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import Chat from './Chat';
const native = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), handlers: new Map<string, (event: { payload: unknown }) => void>() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: native.listen }));
const connected = { generation: 1, phase: 'connected', route: 'direct' };
beforeEach(() => {
  native.handlers.clear();
  native.invoke.mockImplementation(async (command: string) => command === 'get_session_state' ? connected : command === 'drain_messages' ? [] : true);
  native.listen.mockImplementation(async (name: string, callback: (event: { payload: unknown }) => void) => {
    native.handlers.set(name, callback);
    return vi.fn();
  });
});
describe('Chat', () => {
  it('keeps text and marks failure when sending fails', async () => {
    native.invoke.mockImplementation(async (command: string) => {
      if (command === 'get_session_state') return connected;
      if (command === 'drain_messages') return [];
      throw { code: 'send_failed', message: 'Не удалось передать сообщение', stage: 'send', retryable: true };
    });
    render(<Chat onBack={vi.fn()} />);
    const input = screen.getByLabelText('Сообщение');
    await waitFor(() => expect(input).toBeEnabled());
    fireEvent.change(input, { target: { value: 'Не терять этот текст' } });
    fireEvent.click(screen.getByRole('button', { name: 'Отправить' }));
    await screen.findByText('Не удалось передать сообщение');
    expect(input).toHaveValue('Не терять этот текст');
    expect(screen.getByText(/Не отправлено полностью/)).toBeInTheDocument();
    expect(native.invoke).not.toHaveBeenCalledWith('confirm_sas');
  });
  it('sends one complete long message and labels only transport acceptance', async () => {
    render(<Chat onBack={vi.fn()} />);
    const input = screen.getByLabelText('Сообщение');
    await waitFor(() => expect(input).toBeEnabled());
    const text = 'Привет 🌍 '.repeat(1500);
    fireEvent.change(input, { target: { value: text } });
    fireEvent.click(screen.getByRole('button', { name: 'Отправить' }));
    await screen.findByText(/Передано каналу/);
    expect(native.invoke).toHaveBeenCalledWith('send_text', { text });
    expect(native.invoke.mock.calls.filter(([command]) => command === 'send_text')).toHaveLength(1);
    expect(input).toHaveValue('');
  });
  it('drains a message accepted before the chat mounted exactly once', async () => {
    let inbox = [{ id: 'before-mount', generation: 1, text: 'раннее сообщение' }];
    native.invoke.mockImplementation(async (command: string) => {
      if (command === 'get_session_state') return connected;
      if (command === 'drain_messages') { const pending = inbox; inbox = []; return pending; }
      return true;
    });
    render(<Chat onBack={vi.fn()} />);
    await screen.findByText('раннее сообщение');
    act(() => native.handlers.get('ssc-chat-message')?.({ payload: { generation: 1 } }));
    await waitFor(() => expect(screen.getAllByText('раннее сообщение')).toHaveLength(1));
  });
  it('waits for peer readiness and clears history only at final close', async () => {
    render(<Chat onBack={vi.fn()} />);
    await waitFor(() => expect(screen.getByLabelText('Сообщение')).toBeEnabled());
    native.invoke.mockImplementation(async (command: string) => command === 'drain_messages' ? [{ id: 'in-1', generation: 1, text: 'входящее' }] : connected);
    act(() => native.handlers.get('ssc-chat-message')?.({ payload: { generation: 1 } }));
    await screen.findByText('входящее');
    act(() => native.handlers.get('ssc-session')?.({ payload: { ...connected, phase: 'recovering' } }));
    expect(screen.getByText('входящее')).toBeInTheDocument();
    expect(screen.getByLabelText('Сообщение')).toBeDisabled();
    act(() => native.handlers.get('ssc-session')?.({ payload: { ...connected, phase: 'closed' } }));
    expect(screen.queryByText('входящее')).not.toBeInTheDocument();
  });
});
