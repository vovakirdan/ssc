import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { errorMessage, phaseLabels, useSession } from '@/lib/session';

interface Props { onConfirm: () => void; onCancel: () => void; initialSas?: string | null }
export default function VerifyFingerprint({ onConfirm, onCancel, initialSas }: Props) {
  const [fingerprint, setFingerprint] = useState(initialSas ?? '');
  const [checked, setChecked] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { session, error: sessionError } = useSession();
  useEffect(() => {
    let disposed = false;
    void invoke<string | null>('get_fingerprint').then(code => { if (!disposed && code) setFingerprint(code); })
      .catch(failure => { if (!disposed) setError(errorMessage(failure)); });
    return () => { disposed = true; };
  }, []);
  useEffect(() => { if (session.phase === 'connected') onConfirm(); }, [session.phase, onConfirm]);
  async function confirm() {
    if (!fingerprint || !checked || confirming) return;
    setConfirming(true);
    try {
      const accepted = await invoke<boolean>('confirm_sas');
      if (!accepted) throw new Error('Не удалось подтвердить код');
    } catch (failure) { setError(errorMessage(failure)); setConfirming(false); }
  }
  async function cancel() {
    try { await invoke('reject_sas'); onCancel(); }
    catch (failure) { setError(errorMessage(failure)); }
  }
  const waiting = confirming || session.phase === 'waiting_peer';
  const closed = session.phase === 'closed';
  return <div className="min-h-screen bg-slate-900 text-white flex items-center justify-center p-4"><div className="max-w-md w-full space-y-6">
    <h1 className="text-2xl font-bold text-center">Сверка отпечатков</h1>
    <p>Оба устройства должны показывать одинаковый код. Сверьте его с собеседником перед продолжением.</p>
    <p className="text-center font-mono text-2xl bg-slate-800 rounded p-5">{fingerprint || 'Ожидание кода…'}</p>
    {(error || sessionError || closed) && <p role="alert" className="text-red-300">{error || sessionError || 'Соединение разорвано'}</p>}
    {waiting && !closed && <p role="status">{phaseLabels.waiting_peer}</p>}
    <label className="flex gap-2"><input type="checkbox" checked={checked} disabled={waiting || closed} onChange={event => setChecked(event.target.checked)} />Я вижу одинаковый код на обоих устройствах</label>
    <Button className="w-full" disabled={!fingerprint || !checked || waiting || closed} onClick={confirm}>Коды совпадают — продолжить</Button>
    <Button className="w-full" variant="outline" onClick={cancel}>Коды не совпадают / Назад</Button>
  </div></div>;
}
