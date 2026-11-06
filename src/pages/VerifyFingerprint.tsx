import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { errorMessage, phaseLabels, type SessionState, type VerificationCode } from '@/lib/session';

interface Props {
  onConfirm: () => void;
  onCancel: () => void;
  session: SessionState;
  sessionError?: string | null;
  initialSas?: VerificationCode | null;
}

export default function VerifyFingerprint(props: Props) {
  // SAS and session notifications can arrive in either order. A SAS from a
  // newer generation must never inherit the previous session's connected phase.
  const generation = Math.max(props.session.generation, props.initialSas?.generation ?? 0);
  const session: SessionState = props.session.generation === generation ? props.session
    : { generation, phase: 'verification', route: null };
  const initialSas = props.initialSas?.generation === generation ? props.initialSas : null;
  return <VerificationSession key={`${generation}:${initialSas?.sas ?? ''}`} {...props} session={session} initialSas={initialSas} />;
}

function VerificationSession({ initialSas, ...props }: Props) {
  const [code, setCode] = useState<VerificationCode | null>(initialSas ?? null);
  const [error, setError] = useState<string | null>(null);
  const generation = props.session.generation;
  useEffect(() => {
    let disposed = false;
    void invoke<VerificationCode | null>('get_fingerprint').then(code => {
      if (!disposed && code?.generation === generation) setCode(code);
    }).catch(failure => { if (!disposed) setError(errorMessage(failure)); });
    return () => { disposed = true; };
  }, [generation]);
  // Approval belongs to exactly the code that was compared, even if the native
  // session changes while a fingerprint request or a confirmation is in flight.
  return <VerificationView key={`${generation}:${code?.sas ?? ''}`} {...props} code={code} fingerprintError={error} />;
}

function VerificationView({ onConfirm, onCancel, session, sessionError, code, fingerprintError }: Omit<Props, 'initialSas'> & {
  code: VerificationCode | null;
  fingerprintError: string | null;
}) {
  const [checked, setChecked] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  useEffect(() => { if (session.phase === 'connected') onConfirm(); }, [session.phase, onConfirm]);
  async function confirm() {
    if (!code || !checked || confirming || session.phase !== 'verification') return;
    setConfirming(true);
    try {
      const accepted = await invoke<boolean>('confirm_sas', { generation: code.generation, sas: code.sas });
      if (!accepted) throw new Error('Не удалось подтвердить код');
    } catch (failure) {
      if (mounted.current) { setError(errorMessage(failure)); setConfirming(false); }
    }
  }
  async function cancel() {
    try {
      await invoke('reject_sas', { generation: session.generation });
      if (mounted.current) onCancel();
    } catch (failure) { if (mounted.current) setError(errorMessage(failure)); }
  }
  const waiting = confirming || session.phase === 'waiting_peer';
  const closed = session.phase === 'closed';
  const canCompare = !!code && session.phase === 'verification' && !waiting;
  return <div className="min-h-screen bg-slate-900 text-white flex items-center justify-center p-4"><div className="max-w-md w-full space-y-6">
    <h1 className="text-2xl font-bold text-center">Сверка отпечатков</h1>
    <p>Оба устройства должны показывать одинаковый код. Сверьте его с собеседником перед продолжением.</p>
    <p className="text-center font-mono text-2xl bg-slate-800 rounded p-5">{code?.sas || 'Ожидание кода…'}</p>
    {(error || fingerprintError || sessionError || closed) && <p role="alert" className="text-red-300">{error || fingerprintError || sessionError || 'Соединение разорвано'}</p>}
    {waiting && !closed && <p role="status">{phaseLabels.waiting_peer}</p>}
    <label className="flex gap-2"><input type="checkbox" checked={checked} disabled={!canCompare} onChange={event => setChecked(event.target.checked)} />Я вижу одинаковый код на обоих устройствах</label>
    <Button className="w-full" disabled={!canCompare || !checked} onClick={confirm}>Коды совпадают — продолжить</Button>
    <Button className="w-full" variant="outline" onClick={cancel}>Коды не совпадают / Назад</Button>
  </div></div>;
}
