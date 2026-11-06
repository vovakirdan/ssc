import { useEffect, useRef, useState, type ChangeEvent } from 'react';
import { ArrowLeft, Check, Copy, Download, ImagePlus, ShieldCheck } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { QRCodeSVG, QRCodeCanvas } from 'qrcode.react';
import jsQR from 'jsqr';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { errorMessage } from '@/lib/session';

interface ScanQRProps { onBack: () => void; onConnected: () => void }
const MAX_QR_FILE_BYTES = 5 * 1024 * 1024;
const MAX_IMAGE_PIXELS = 16 * 1024 * 1024;

export default function ScanQR({ onBack }: ScanQRProps) {
  const [offerInput, setOfferInput] = useState('');
  const [answer, setAnswer] = useState('');
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);
  const [fileLoading, setFileLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);
  const objectURLs = useRef(new Set<string>());
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    mounted.current = true;
    const urls = objectURLs.current;
    return () => { mounted.current = false; urls.forEach(url => URL.revokeObjectURL(url)); urls.clear(); };
  }, []);

  async function acceptOffer() {
    if (!offerInput.trim() || loading || fileLoading) return;
    setLoading(true); setError(null); setAnswer(''); setCopied(false);
    try {
      const result = await invoke<string>('accept_offer_with_candidates', { encoded: offerInput.trim() });
      if (mounted.current) setAnswer(result);
    } catch (failure) {
      if (mounted.current) setError(errorMessage(failure, 'Не удалось принять приглашение. Проверьте полученный код.'));
    } finally { if (mounted.current) setLoading(false); }
  }
  async function copyAnswer() {
    try { await navigator.clipboard.writeText(answer); if (mounted.current) { setCopied(true); setError(null); } }
    catch { if (mounted.current) setError('Не удалось скопировать ответ. Выделите код и скопируйте вручную.'); }
  }
  function saveQR() {
    if (!canvas.current) return;
    const link = document.createElement('a');
    link.href = canvas.current.toDataURL('image/png'); link.download = 'ssc-answer.png'; link.click();
  }
  function importQR(event: ChangeEvent<HTMLInputElement>) {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = '';
    if (!file) return;
    setError(null);
    if (file.size > MAX_QR_FILE_BYTES) { setError('Изображение слишком большое. Выберите файл до 5 MiB.'); return; }
    if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type)) { setError('Выберите изображение PNG, JPEG или WEBP.'); return; }
    setFileLoading(true);
    let url: string;
    try { url = URL.createObjectURL(file); }
    catch { setFileLoading(false); setError('Не удалось открыть изображение. Попробуйте другой файл.'); return; }
    objectURLs.current.add(url);
    const image = new window.Image();
    function finish() { URL.revokeObjectURL(url); objectURLs.current.delete(url); if (mounted.current) setFileLoading(false); }
    image.onload = () => {
      try {
        if (!mounted.current) return;
        if (!image.width || !image.height || image.width * image.height > MAX_IMAGE_PIXELS) throw new Error('image_size');
        const reader = document.createElement('canvas'); reader.width = image.width; reader.height = image.height;
        const context = reader.getContext('2d');
        if (!context) throw new Error('canvas');
        context.drawImage(image, 0, 0);
        const pixels = context.getImageData(0, 0, image.width, image.height);
        const code = jsQR(pixels.data, image.width, image.height);
        if (!code?.data) { setError('QR-код не найден. Попробуйте другое изображение или вставьте код вручную.'); return; }
        setOfferInput(code.data); setAnswer(''); setCopied(false);
      } catch { if (mounted.current) setError('Не удалось прочитать изображение. Выберите изображение поменьше или вставьте код вручную.'); }
      finally { finish(); }
    };
    image.onerror = () => { if (mounted.current) setError('Не удалось прочитать изображение. Попробуйте другой файл.'); finish(); };
    image.src = url;
  }

  return <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-800 text-white p-5 sm:p-8">
    <main className="max-w-xl mx-auto space-y-6">
      <Button variant="ghost" onClick={onBack} className="text-slate-300 -ml-3"><ArrowLeft className="w-4 h-4 mr-2" />Назад</Button>
      <header><p className="text-emerald-400 text-xs uppercase tracking-[0.2em] mb-3">Меня пригласили</p><h1 className="text-3xl font-semibold tracking-tight">Присоединиться к чату</h1><p className="text-slate-400 mt-3">Примите приглашение и отправьте свой ответ тому, кто создал чат.</p></header>
      <section className="rounded-2xl border border-slate-700 bg-slate-900/70 p-5 sm:p-6 space-y-4" aria-labelledby="paste-step">
        <h2 id="paste-step" className="text-lg font-semibold">1. Вставьте приглашение</h2>
        <label htmlFor="received-invitation" className="block text-sm text-slate-400">Код, который прислал собеседник</label>
        <Textarea id="received-invitation" placeholder="Вставьте код приглашения..." value={offerInput} onChange={event => { setOfferInput(event.target.value); setAnswer(''); setCopied(false); setError(null); }} disabled={loading} className="bg-slate-800 border-slate-600 text-white min-h-[110px] font-mono text-sm" />
        <Button onClick={acceptOffer} disabled={loading || fileLoading || !offerInput.trim()} className="w-full bg-emerald-600 hover:bg-emerald-500">{loading ? 'Принимаем приглашение…' : 'Принять приглашение'}</Button>
        <div className="border-t border-slate-700 pt-4 space-y-3">
          <label className="flex gap-2 text-sm text-slate-300 items-center" htmlFor="qr-image"><ImagePlus className="w-4 h-4" />Или загрузите изображение QR</label>
          <input id="qr-image" type="file" accept="image/png,image/jpeg,image/webp" disabled={fileLoading || loading} onChange={importQR} className="block w-full text-xs text-slate-400 file:mr-3 file:py-2 file:px-3 file:rounded-lg file:border file:border-slate-600 file:bg-slate-800 file:text-slate-200" />
          <p className="text-xs text-slate-500">PNG, JPEG или WEBP, до 5 MiB. Сканирование камерой пока недоступно.</p>
          {fileLoading && <p role="status" className="text-sm text-slate-300">Читаем QR-код…</p>}
        </div>
      </section>
      {error && <p role="alert" className="text-sm text-red-300">{error}</p>}
      {answer && <section className="rounded-2xl border border-emerald-800/70 bg-emerald-950/20 p-5 sm:p-6 space-y-4" aria-labelledby="reply-step">
        <h2 id="reply-step" className="text-lg font-semibold">2. Отправьте ответ собеседнику</h2>
        <p className="text-sm text-slate-400">Верните этот код человеку, который прислал приглашение. Он вставит его у себя в приложении.</p>
        <div className="rounded-xl bg-white p-4 max-w-[200px] mx-auto"><QRCodeSVG value={answer} className="w-full h-auto" size={256} /></div>
        <div className="hidden"><QRCodeCanvas ref={canvas} value={answer} size={512} /></div>
        <label htmlFor="answer-code" className="block text-sm text-slate-300">Код ответа</label>
        <Input id="answer-code" aria-label="Код ответа" value={answer} readOnly className="bg-slate-800 border-slate-600 text-white font-mono text-xs" />
        <div className="flex flex-wrap gap-2"><Button onClick={copyAnswer} className="bg-emerald-600 hover:bg-emerald-500">{copied ? <Check className="w-4 h-4 mr-2" /> : <Copy className="w-4 h-4 mr-2" />}{copied ? 'Ответ скопирован' : 'Скопировать ответ'}</Button><Button variant="ghost" onClick={saveQR} className="text-slate-300"><Download className="w-4 h-4 mr-2" />Сохранить QR</Button></div>
      </section>}
      <p className="flex items-start gap-2 text-sm text-slate-400"><ShieldCheck className="w-4 h-4 flex-shrink-0 mt-0.5" />После обмена появится код сверки. Сравните его на обоих устройствах и подтвердите совпадение.</p>
    </main>
  </div>;
}
