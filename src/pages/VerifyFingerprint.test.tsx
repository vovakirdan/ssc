import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import VerifyFingerprint from './VerifyFingerprint';
const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => vi.fn()) }));
beforeEach(() => native.invoke.mockImplementation(async (name: string) => {
  if (name === 'get_fingerprint') return 'aabbccddeeff';
  if (name === 'get_session_state') return { generation: 1, phase: 'verification', route: null };
  return true;
}));
it('requires explicit user confirmation and keeps waiting after local readiness', async () => {
  const onConfirm = vi.fn();
  render(<VerifyFingerprint onConfirm={onConfirm} onCancel={vi.fn()} />);
  await screen.findByText('aabbccddeeff');
  const button = screen.getByRole('button', { name: 'Коды совпадают — продолжить' });
  expect(button).toBeDisabled();
  fireEvent.click(screen.getByRole('checkbox'));
  fireEvent.click(button);
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('confirm_sas'));
  expect(onConfirm).not.toHaveBeenCalled();
  expect(screen.getByRole('status')).toHaveTextContent('Ожидаем подтверждения собеседника');
});
