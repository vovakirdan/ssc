import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import VerifyFingerprint from './VerifyFingerprint';
import type { SessionState, VerificationCode } from '@/lib/session';

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
const session: SessionState = { generation: 1, phase: 'verification', route: null };
const firstCode: VerificationCode = { generation: 1, sas: 'aabbccddeeff' };
const confirmButton = () => screen.getByRole('button', { name: 'Коды совпадают — продолжить' });
beforeEach(() => { native.invoke.mockReset().mockImplementation(async (name: string) => name === 'get_fingerprint' ? firstCode : true); });

it('requires explicit comparison and sends exactly the displayed generation and code', async () => {
  const onConfirm = vi.fn();
  render(<VerifyFingerprint session={session} onConfirm={onConfirm} onCancel={vi.fn()} />);
  await screen.findByText(firstCode.sas);
  expect(confirmButton()).toBeDisabled();
  fireEvent.click(screen.getByRole('checkbox'));
  fireEvent.click(confirmButton());
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('confirm_sas', firstCode));
  expect(onConfirm).not.toHaveBeenCalled();
  expect(screen.getByRole('status')).toHaveTextContent('Ожидаем подтверждения собеседника');
});

it('resets comparison when an authoritative fingerprint changes within the generation', async () => {
  let resolve: (code: VerificationCode) => void = () => {};
  native.invoke.mockReturnValue(new Promise(done => { resolve = done; }));
  render(<VerifyFingerprint session={session} initialSas={firstCode} onConfirm={vi.fn()} onCancel={vi.fn()} />);
  fireEvent.click(screen.getByRole('checkbox'));
  expect(confirmButton()).toBeEnabled();
  await act(async () => resolve({ generation: 1, sas: '112233445566' }));
  expect(screen.getByText('112233445566')).toBeInTheDocument();
  expect(screen.getByRole('checkbox')).not.toBeChecked();
  expect(confirmButton()).toBeDisabled();
});

it('resets code and approval for a new generation and ignores the old deferred fingerprint', async () => {
  const replies: Array<(code: VerificationCode) => void> = [];
  native.invoke.mockImplementation(() => new Promise(done => { replies.push(done); }));
  const { rerender } = render(<VerifyFingerprint session={session} initialSas={firstCode} onConfirm={vi.fn()} onCancel={vi.fn()} />);
  fireEvent.click(screen.getByRole('checkbox'));
  rerender(<VerifyFingerprint session={{ ...session, generation: 2 }} initialSas={firstCode} onConfirm={vi.fn()} onCancel={vi.fn()} />);
  expect(screen.queryByText(firstCode.sas)).not.toBeInTheDocument();
  expect(screen.getByText('Ожидание кода…')).toBeInTheDocument();
  expect(screen.getByRole('checkbox')).not.toBeChecked();
  await act(async () => replies[0](firstCode));
  expect(screen.queryByText(firstCode.sas)).not.toBeInTheDocument();
  await act(async () => replies[1]({ generation: 2, sas: 'bbbb33334444' }));
  expect(screen.getByText('bbbb33334444')).toBeInTheDocument();
  expect(confirmButton()).toBeDisabled();
});

it('replaces event codes without retaining pending confirmation or accepting a stale response', async () => {
  let oldConfirmation: (accepted: boolean) => void = () => {};
  let fingerprint: (code: VerificationCode) => void = () => {};
  native.invoke.mockImplementation((name: string) => new Promise(done => {
    if (name === 'get_fingerprint') fingerprint = done;
    else oldConfirmation = done;
  }));
  const props = { session, onConfirm: vi.fn(), onCancel: vi.fn() };
  const { rerender } = render(<VerifyFingerprint {...props} initialSas={firstCode} />);
  const oldFingerprint = fingerprint;
  fireEvent.click(screen.getByRole('checkbox'));
  fireEvent.click(confirmButton());
  const nextCode = { generation: 1, sas: 'cccc55556666' };
  rerender(<VerifyFingerprint {...props} initialSas={nextCode} />);
  await act(async () => { oldFingerprint(firstCode); oldConfirmation(true); });
  expect(screen.getByText(nextCode.sas)).toBeInTheDocument();
  expect(screen.getByRole('checkbox')).not.toBeChecked();
  expect(screen.queryByRole('status')).not.toBeInTheDocument();
  expect(confirmButton()).toBeDisabled();
});

it('rejects only the displayed generation and opens chat only after its connected state', async () => {
  const onConfirm = vi.fn(), onCancel = vi.fn();
  const { rerender } = render(<VerifyFingerprint session={session} initialSas={firstCode} onConfirm={onConfirm} onCancel={onCancel} />);
  fireEvent.click(screen.getByRole('button', { name: 'Коды не совпадают / Назад' }));
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('reject_sas', { generation: 1 }));
  expect(onCancel).toHaveBeenCalledOnce();
  expect(onConfirm).not.toHaveBeenCalled();
  rerender(<VerifyFingerprint session={{ ...session, phase: 'connected' }} initialSas={firstCode} onConfirm={onConfirm} onCancel={onCancel} />);
  await waitFor(() => expect(onConfirm).toHaveBeenCalledOnce());
});

it('does not treat the previous connected phase as confirmation of a newer SAS arriving first', async () => {
  native.invoke.mockResolvedValue(firstCode);
  const onConfirm = vi.fn();
  const nextCode = { generation: 2, sas: 'dddd77778888' };
  const { rerender } = render(<VerifyFingerprint session={{ ...session, phase: 'connected' }} initialSas={nextCode} onConfirm={onConfirm} onCancel={vi.fn()} />);
  await act(async () => {});
  expect(onConfirm).not.toHaveBeenCalled();
  expect(screen.getByText(nextCode.sas)).toBeInTheDocument();
  expect(screen.getByRole('checkbox')).not.toBeChecked();
  expect(confirmButton()).toBeDisabled();
  rerender(<VerifyFingerprint session={{ ...session, generation: 2, phase: 'connected' }} initialSas={nextCode} onConfirm={onConfirm} onCancel={vi.fn()} />);
  expect(onConfirm).toHaveBeenCalledOnce();
});
