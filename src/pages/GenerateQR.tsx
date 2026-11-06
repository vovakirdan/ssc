import { useState, useEffect, useRef, useCallback } from 'react';
import { ArrowLeft, Check, Copy, Download, Loader2, RefreshCw, ShieldCheck } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { QRCodeSVG, QRCodeCanvas } from 'qrcode.react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { errorMessage } from '@/lib/session';

interface GenerateQRProps {
  onBack: () => void;
  onConnected: () => void;
  autoGenerate?: boolean;
  ttl?: number;
}

export default function GenerateQR({ onBack, autoGenerate, ttl: ttlMinutes = 5 }: GenerateQRProps) {
  const [offer, setOffer] = useState('');
  const [generating, setGenerating] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [copied, setCopied] = useState(false);
  const [answer, setAnswer] = useState('');
  const [answerAccepted, setAnswerAccepted] = useState(false);
  const [generationError, setGenerationError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const lifetime = Math.max(1, ttlMinutes) * 60;
  const [remaining, setRemaining] = useState(lifetime);
  const generatingRef = useRef(false);
  const mounted = useRef(true);
  const canvas = useRef<HTMLCanvasElement>(null);

  const generateOffer = useCallback(async () => {
    if (generatingRef.current) return;
    generatingRef.current = true;
    setRemaining(lifetime); setGenerating(true); setGenerationError(null); setActionError(null);
    setOffer(''); setAnswer(''); setCopied(false); setAnswerAccepted(false);
    try {
      const result = await invoke<string>('generate_offer_with_candidates');
      if (mounted.current) setOffer(result);
    } catch (failure) {
      if (mounted.current) setGenerationError(errorMessage(failure, 'Не удалось создать приглашение. Попробуйте ещё раз.'));
    } finally {
      generatingRef.current = false;
      if (mounted.current) setGenerating(false);
    }
  }, [lifetime]);
  useEffect(() => {
    mounted.current = true;
    if (autoGenerate) void generateOffer();
    return () => { mounted.current = false; };
  }, [autoGenerate, generateOffer]);
  useEffect(() => {
    setRemaining(lifetime);
    if (!offer || answerAccepted) return;
    const timer = setInterval(() => setRemaining(value => Math.max(0, value - 1)), 1000);
    return () => clearInterval(timer);
  }, [offer, lifetime, answerAccepted]);
  useEffect(() => {
    if (offer && remaining === 0 && !answerAccepted) void generateOffer();
  }, [offer, remaining, answerAccepted, generateOffer]);

  async function copy() {
    try { await navigator.clipboard.writeText(offer); if (mounted.current) { setCopied(true); setActionError(null); } }
    catch { if (mounted.current) setActionError('Не удалось скопировать код. Выделите его и скопируйте вручную.'); }
  }
  async function acceptAnswer() {
    if (!answer.trim() || submitting || generating) return;
    setSubmitting(true); setActionError(null);
    try {
      const accepted = await invoke<boolean>('set_answer_with_candidates', { encoded: answer.trim() });
      if (!accepted) throw new Error('Не удалось принять ответ. Проверьте полученный код.');
      if (mounted.current) setAnswerAccepted(true);
    } catch (failure) {
      if (mounted.current) setActionError(errorMessage(failure, 'Не удалось принять ответ. Проверьте полученный код.'));
    } finally { if (mounted.current) setSubmitting(false); }
  }
  function saveQR() {
    if (!canvas.current) return;
    const link = document.createElement('a');
    link.href = canvas.current.toDataURL('image/png'); link.download = 'ssc-invitation.png'; link.click();
  }

  return <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-800 text-white p-5 sm:p-8">
    <main className="max-w-xl mx-auto space-y-6">
      <Button variant="ghost" onClick={onBack} className="text-slate-300 -ml-3"><ArrowLeft className="w-4 h-4 mr-2" />Назад</Button>
      <header><p className="text-emerald-400 text-xs uppercase tracking-[0.2em] mb-3">Я приглашаю</p><h1 className="text-3xl font-semibold tracking-tight">Создать подключение</h1><p className="text-slate-400 mt-3">Обменяйтесь кодами с собеседником, затем сверьте отпечатки.</p></header>
      <section className="rounded-2xl border border-slate-700 bg-slate-900/70 p-5 sm:p-6 space-y-4" aria-labelledby="invitation-step">
        <h2 id="invitation-step" className="text-lg font-semibold">1. Отправьте приглашение</h2>
        <p className="text-slate-400 text-sm">Собеседник может вставить код в приложение или загрузить изображение QR.</p>
        {generating && <div role="status" className="flex items-center justify-center gap-3 py-12 text-slate-300"><Loader2 className="w-5 h-5 animate-spin" />Создаём приглашение…</div>}
        {generationError && <div className="space-y-3"><p role="alert" className="text-red-300 text-sm">{generationError}</p><Button onClick={generateOffer}><RefreshCw className="w-4 h-4 mr-2" />Повторить создание</Button></div>}
        {!offer && !generating && !generationError && <Button onClick={generateOffer}>Создать приглашение</Button>}
        {offer && <>
          <label className="block text-sm text-slate-300" htmlFor="invitation-code">Код приглашения</label>
          <div className="flex gap-2"><Input id="invitation-code" aria-label="Код приглашения" value={offer} readOnly className="bg-slate-800 border-slate-600 text-white font-mono text-xs" /><Button onClick={copy} aria-label="Скопировать приглашение">{copied ? <Check className="w-4 h-4" /> : <Copy className="w-4 h-4" />}</Button></div>
          {copied && <p role="status" className="text-xs text-emerald-400">Код приглашения скопирован</p>}
          <div className="rounded-xl bg-white p-4 max-w-[200px] mx-auto"><QRCodeSVG value={offer} className="w-full h-auto" size={256} /></div>
          <div className="hidden"><QRCodeCanvas ref={canvas} value={offer} size={512} /></div>
          {!answerAccepted && <p className="text-center text-xs text-slate-400">Приглашение обновится через {Math.floor(remaining / 60)}:{String(remaining % 60).padStart(2, '0')}</p>}
          <Button variant="ghost" onClick={saveQR} className="text-slate-300"><Download className="w-4 h-4 mr-2" />Сохранить QR в PNG</Button>
        </>}
      </section>
      <section className={`rounded-2xl border border-slate-700 bg-slate-900/70 p-5 sm:p-6 space-y-4 ${!offer ? 'opacity-60' : ''}`} aria-labelledby="answer-step">
        <h2 id="answer-step" className="text-lg font-semibold">2. Вставьте ответ собеседника</h2>
        <p className="text-slate-400 text-sm">Попросите собеседника принять приглашение и прислать вам ответный код.</p>
        <Input aria-label="Ответ собеседника" placeholder="Вставьте ответ здесь..." value={answer} onChange={event => setAnswer(event.target.value)} disabled={!offer || submitting || answerAccepted} className="bg-slate-800 border-slate-600 text-white" />
        <Button onClick={acceptAnswer} disabled={!offer || !answer.trim() || submitting || answerAccepted} className="w-full bg-emerald-600 hover:bg-emerald-500">{submitting ? 'Принимаем ответ…' : answerAccepted ? 'Ответ принят' : 'Подключиться'}</Button>
        {answerAccepted && <p role="status" className="text-sm text-emerald-300">Ответ принят. Устанавливаем защищённое соединение…</p>}
      </section>
      {actionError && <p role="alert" className="text-red-300 text-sm">{actionError}</p>}
      <p className="flex items-start gap-2 text-sm text-slate-400"><ShieldCheck className="w-4 h-4 flex-shrink-0 mt-0.5" />Когда появится код сверки, сравните его с собеседником. Чат станет доступен после подтверждения с обеих сторон.</p>
    </main>
  </div>;
}
