import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import ScanQR from './ScanQR';
const native = vi.hoisted(() => ({ invoke: vi.fn(), copy: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('qrcode.react', async () => {
  const { forwardRef } = await import('react');
  return { QRCodeSVG: () => <svg aria-label="QR ответа" />, QRCodeCanvas: forwardRef<HTMLCanvasElement>((_props, ref) => <canvas ref={ref} />) };
});
beforeEach(() => { native.invoke.mockReset(); native.copy.mockReset().mockResolvedValue(undefined); Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: native.copy } }); });
it('accepts a pasted code then lets the recipient copy the actual answer', async () => {
  native.invoke.mockResolvedValue('v2:real-answer');
  const onBack = vi.fn(), onConnected = vi.fn();
  render(<ScanQR onBack={onBack} onConnected={onConnected} />);
  fireEvent.change(screen.getByRole('textbox', { name: 'Код, который прислал собеседник' }), { target: { value: ' v2:offer ' } });
  fireEvent.click(screen.getByRole('button', { name: 'Принять приглашение' }));
  expect(await screen.findByLabelText('Код ответа')).toHaveValue('v2:real-answer');
  expect(native.invoke).toHaveBeenCalledWith('accept_offer_with_candidates', { encoded: 'v2:offer' });
  fireEvent.click(screen.getByRole('button', { name: 'Скопировать ответ' }));
  await waitFor(() => expect(native.copy).toHaveBeenCalledWith('v2:real-answer'));
  expect(onConnected).not.toHaveBeenCalled();
  fireEvent.change(screen.getByRole('textbox', { name: 'Код, который прислал собеседник' }), { target: { value: 'v2:different-offer' } });
  expect(screen.queryByLabelText('Код ответа')).not.toBeInTheDocument();
  expect(screen.queryByText('Ответ скопирован')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Назад' })); expect(onBack).toHaveBeenCalledOnce();
});
it('keeps the pasted invitation when accepting fails', async () => {
  native.invoke.mockRejectedValue({ message: 'Приглашение повреждено' });
  render(<ScanQR onBack={vi.fn()} onConnected={vi.fn()} />);
  const input = screen.getByRole('textbox');
  fireEvent.change(input, { target: { value: 'bad-code' } });
  fireEvent.click(screen.getByRole('button', { name: 'Принять приглашение' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Приглашение повреждено');
  expect(input).toHaveValue('bad-code'); expect(screen.queryByLabelText('Код ответа')).not.toBeInTheDocument();
});
it('rejects an oversized QR file before reading it and shows the camera limitation honestly', async () => {
  render(<ScanQR onBack={vi.fn()} onConnected={vi.fn()} />);
  const file = new File([new Uint8Array(5 * 1024 * 1024 + 1)], 'large.png', { type: 'image/png' });
  fireEvent.change(screen.getByLabelText('Или загрузите изображение QR'), { target: { files: [file] } });
  expect(screen.getByRole('alert')).toHaveTextContent('до 5 MiB');
  expect(native.invoke).not.toHaveBeenCalled();
  expect(screen.getByText(/Сканирование камерой пока недоступно/)).toBeInTheDocument();
});
