import { useCallback, useEffect, useRef, useState } from 'react';
import { VoiceCapture, type VoiceDraft } from './recorder';
export function useVoice() {
  const [phase, setPhase] = useState<'idle' | 'permission' | 'recording' | 'stopping' | 'preview'>('idle');
  const [draft, setDraft] = useState<(VoiceDraft & { url: string }) | null>(null);
  const [elapsed, setElapsed] = useState(0), [error, setError] = useState<string | null>(null);
  const operation = useRef(0);
  const mounted = useRef(true), preview = useRef<string | null>(null), started = useRef(0);
  const complete = useCallback((value: VoiceDraft | null) => {
    if (!mounted.current) return;
    if (!value) { setPhase('idle'); setError('Не удалось получить запись. Попробуйте ещё раз.'); return; }
    if (preview.current) URL.revokeObjectURL(preview.current);
    const url = URL.createObjectURL(value.blob); preview.current = url; setDraft({ ...value, url }); setPhase('preview');
  }, []);
  const capture = useRef<VoiceCapture | null>(null);
  if (!capture.current) capture.current = new VoiceCapture(complete);
  const discard = useCallback(() => {
    operation.current++; capture.current?.cancel(); if (preview.current) URL.revokeObjectURL(preview.current); preview.current = null;
    if (mounted.current) { setDraft(null); setPhase('idle'); setElapsed(0); setError(null); }
  }, []);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; capture.current?.cancel(); if (preview.current) URL.revokeObjectURL(preview.current); preview.current = null; };
  }, []);
  useEffect(() => {
    if (phase !== 'recording') return;
    const timer = setInterval(() => setElapsed(Math.min(300, Math.floor((performance.now() - started.current) / 1000))), 250);
    return () => clearInterval(timer);
  }, [phase]);
  const start = async (peer: readonly string[]) => {
    if (phase !== 'idle') return;
    const epoch = ++operation.current;
    setPhase('permission'); setError(null);
    try { if (await capture.current!.start(peer)) { if (mounted.current && epoch === operation.current) { started.current = performance.now(); setPhase('recording'); } } else if (mounted.current && epoch === operation.current) setPhase('idle'); }
    catch { if (mounted.current && epoch === operation.current) { setError('Микрофон недоступен. Проверьте разрешение приложения и браузера.'); setPhase('idle'); } }
  };
  const stop = async () => {
    if (phase !== 'recording') return; const epoch = operation.current; setPhase('stopping');
    try { const value = await capture.current!.stop(); if (epoch === operation.current) complete(value); }
    catch { if (mounted.current && epoch === operation.current) { discard(); setError('Не удалось завершить запись. Попробуйте снова.'); } }
  };
  return { phase, draft, elapsed, error, start, stop, discard };
}
