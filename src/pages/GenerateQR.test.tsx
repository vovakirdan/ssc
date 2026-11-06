import { StrictMode } from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import GenerateQR from './GenerateQR';
const native = vi.hoisted(() => ({ invoke: vi.fn(), copy: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('qrcode.react', async () => {
  const { forwardRef } = await import('react');
  return { QRCodeSVG: () => <svg aria-label="QR приглашения" />, QRCodeCanvas: forwardRef<HTMLCanvasElement>((_props, ref) => <canvas ref={ref} />) };
});
beforeEach(() => { native.invoke.mockReset(); native.copy.mockReset().mockResolvedValue(undefined); Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: native.copy } }); });
it('does not start concurrent generation under StrictMode', () => {
  native.invoke.mockReturnValue(new Promise(() => {}));
  const { unmount } = render(<StrictMode><GenerateQR onBack={vi.fn()} onConnected={vi.fn()} autoGenerate /></StrictMode>);
  expect(native.invoke).toHaveBeenCalledTimes(1);
  expect(native.invoke).toHaveBeenCalledWith('generate_offer_with_candidates');
  unmount();
});
it('shows an inline failure and retries instead of leaving an endless spinner', async () => {
  native.invoke.mockRejectedValueOnce({ message: 'Не удалось собрать ICE' }).mockResolvedValueOnce('v2:offer');
  render(<GenerateQR onBack={vi.fn()} onConnected={vi.fn()} autoGenerate />);
  expect(await screen.findByRole('alert')).toHaveTextContent('Не удалось собрать ICE');
  expect(screen.queryByText('Создаём приглашение…')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Повторить создание' }));
  expect(await screen.findByLabelText('Код приглашения')).toHaveValue('v2:offer');
  expect(native.invoke).toHaveBeenCalledTimes(2);
});
it('copies the invitation code and accepts the pasted answer without claiming chat readiness', async () => {
  native.invoke.mockImplementation(async (command: string) => command === 'generate_offer_with_candidates' ? 'v2:exact-offer' : true);
  const onConnected = vi.fn(), onBack = vi.fn();
  render(<GenerateQR onBack={onBack} onConnected={onConnected} autoGenerate />);
  await screen.findByLabelText('Код приглашения');
  fireEvent.click(screen.getByRole('button', { name: 'Скопировать приглашение' }));
  await waitFor(() => expect(native.copy).toHaveBeenCalledWith('v2:exact-offer'));
  fireEvent.change(screen.getByPlaceholderText('Вставьте ответ здесь...'), { target: { value: ' v2:answer ' } });
  fireEvent.click(screen.getByRole('button', { name: 'Подключиться' }));
  await screen.findByText('Ответ принят. Устанавливаем защищённое соединение…');
  expect(native.invoke).toHaveBeenCalledWith('set_answer_with_candidates', { encoded: 'v2:answer' });
  expect(onConnected).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Назад' }));
  expect(onBack).toHaveBeenCalledOnce();
});

it('refreshes an expired invitation once and restarts its countdown', async () => {
  vi.useFakeTimers();
  try {
    native.invoke.mockResolvedValue('v2:refreshable-offer');
    const view = render(<GenerateQR onBack={vi.fn()} onConnected={vi.fn()} autoGenerate ttl={1} />);
    await act(async () => { await Promise.resolve(); });
    expect(screen.getByLabelText('Код приглашения')).toHaveValue('v2:refreshable-offer');
    await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
    expect(native.invoke).toHaveBeenCalledTimes(2);
    expect(screen.getByText(/Приглашение обновится через 1:00/)).toBeInTheDocument();
    view.unmount();
  } finally { vi.useRealTimers(); }
});
