import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Check, Copy, Link2, Loader2, RefreshCw, ShieldCheck } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { errorMessage } from '@/lib/session';
import { type LinkSnapshot, linkPhaseLabels, useLinkState } from '@/lib/link';

interface Props { onBack: () => void; ttl: number; temporary?: boolean }

export default function LinkConnection({ onBack, ttl, temporary = false }: Props) {
  const { snapshot, current, eventVersion, applyCommand, error: stateError } = useLinkState();
  const [url, setUrl] = useState('');
  const [pending, setPending] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now);
  const mounted = useRef(true);
  const leaving = useRef(false);
  const pendingCommand = useRef<Promise<LinkSnapshot> | null>(null);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  useEffect(() => {
    if (!snapshot?.expires_at) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [snapshot?.expires_at]);

  const operation = snapshot?.phase === 'cancelled' ? null : snapshot;
  const active = operation !== null && !['error', 'complete'].includes(operation.phase);
  const remaining = operation?.expires_at ? Math.max(0, Math.ceil((Date.parse(operation.expires_at) - now) / 1000)) : null;
  const showCreate = !operation || operation.role === 'creator';
  const showJoin = !operation || operation.role === 'guest';
  const retryingExistingGuest = operation?.role === 'guest' && operation.phase === 'error' && operation.error?.retryable;
  const visibleError = error || operation?.error?.message || stateError;

  async function command(name: string, args: Record<string, unknown>, replacePrevious = false) {
    if (pendingCommand.current || leaving.current) return;
    setPending(true); setError(null); setCopied(false);
    const version = eventVersion.current;
    const request = (async () => {
      if (replacePrevious && current.current && current.current.phase !== 'cancelled') {
        await invoke('cancel_link_invitation', { generation: current.current.generation });
      }
      if (leaving.current) throw new Error('Подключение отменено');
      return invoke<LinkSnapshot>(name, args);
    })();
    pendingCommand.current = request;
    try {
      const next = await request;
      if (mounted.current && !leaving.current) applyCommand(next, version);
    } catch (failure) {
      if (mounted.current && !leaving.current) setError(errorMessage(failure, 'Не удалось выполнить подключение по ссылке'));
    } finally {
      if (pendingCommand.current === request) pendingCommand.current = null;
      if (mounted.current && !leaving.current) setPending(false);
    }
  }
  async function create() {
    await command(temporary ? 'create_temporary_room' : 'create_link_invitation', temporary ? {} : { ttlSeconds: ttl * 60 }, true);
  }
  async function join() {
    if (!url.trim()) return;
    await command('join_link_invitation', { url: url.trim() });
  }
  async function retry() {
    if (!operation?.error?.retryable) return;
    await command('retry_link_invitation', { generation: operation.generation });
  }
  async function back() {
    if (leaving.current) return;
    leaving.current = true; setCancelling(true); setPending(true); setError(null);
    let generation = current.current?.generation;
    try {
      // The command can still be registering a new session when Back is pressed.
      if (pendingCommand.current) {
        try { generation = (await pendingCommand.current).generation; }
        catch { /* No receipt means there is no new generation for this page to cancel. */ }
      }
      if (generation !== undefined) await invoke('cancel_link_invitation', { generation });
      if (mounted.current) onBack();
    } catch (failure) {
      if (mounted.current) { setError(errorMessage(failure, 'Не удалось отменить приглашение')); setPending(false); setCancelling(false); }
      leaving.current = false;
    }
  }
  async function copy() {
    if (!operation?.url) return;
    try { await navigator.clipboard.writeText(operation.url); if (mounted.current) { setCopied(true); setError(null); } }
    catch { if (mounted.current) setError('Не удалось скопировать ссылку. Выделите её и скопируйте вручную.'); }
  }

  return <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-800 text-white p-5 sm:p-8">
    <main className="max-w-xl mx-auto space-y-6">
      <Button variant="ghost" onClick={back} disabled={cancelling} className="text-slate-300 -ml-3"><ArrowLeft className="w-4 h-4 mr-2" />Назад</Button>
      <header className="space-y-3"><p className="text-emerald-400 text-xs uppercase tracking-[0.2em]">По короткой ссылке</p><h1 className="text-3xl font-semibold tracking-tight">Одно приглашение — один чат</h1><p className="text-slate-400">Передайте ссылку собеседнику. Ответ приложение отправит автоматически.</p></header>
      {showCreate && <section className="rounded-2xl border border-emerald-800/70 bg-emerald-950/20 p-5 space-y-4" aria-labelledby="create-link-heading">
        <h2 id="create-link-heading" className="text-lg font-semibold">Я приглашаю</h2>
        {!operation?.url && <p className="text-sm text-slate-400">Ссылка будет действовать {ttl} мин. — как указано в настройках подключения.</p>}
        {(!active || !operation) && <Button onClick={create} disabled={pending} className="bg-emerald-700 hover:bg-emerald-800 text-white"><Link2 className="w-4 h-4 mr-2" />{operation ? 'Создать новую ссылку' : 'Создать ссылку'}</Button>}
        {operation?.url && <>
          <label htmlFor="short-link" className="block text-sm text-slate-300">Передайте эту ссылку собеседнику</label>
          <div className="flex gap-2"><Input id="short-link" aria-label="Короткая ссылка" value={operation.url} readOnly className="bg-slate-800 border-slate-600 text-white font-mono text-xs" /><Button onClick={copy} aria-label="Скопировать ссылку" className="bg-emerald-700 hover:bg-emerald-800 text-white">{copied ? <Check /> : <Copy />}</Button></div>
          {copied && <p className="text-xs text-emerald-300">Ссылка скопирована</p>}
          {remaining !== null && Number.isFinite(remaining) && <p className="text-xs text-slate-400">{remaining > 0 ? `Осталось ${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, '0')}` : 'Срок ссылки истёк. Ожидаем состояние от сервиса.'}</p>}
        </>}
      </section>}
      {showJoin && <section className="rounded-2xl border border-slate-700 bg-slate-900/70 p-5 space-y-4" aria-labelledby="join-link-heading">
        <h2 id="join-link-heading" className="text-lg font-semibold">Меня пригласили</h2>
        <label htmlFor="received-link" className="block text-sm text-slate-400">Вставьте ссылку от собеседника</label>
        <Input id="received-link" aria-label="Ссылка приглашения" value={url} onChange={event => { setUrl(event.target.value); setError(null); }} placeholder="Ссылка приглашения" disabled={active || pending || retryingExistingGuest} className="bg-slate-800 border-slate-600 text-white" />
        <Button onClick={join} disabled={!url.trim() || active || pending || retryingExistingGuest} className="w-full bg-emerald-700 hover:bg-emerald-800 text-white">Подключиться по ссылке</Button>
      </section>}
      {operation && <div role="status" className="flex items-start gap-3 text-sm text-slate-300">{active && <Loader2 className="w-4 h-4 mt-0.5 animate-spin flex-shrink-0" />}<span>{linkPhaseLabels[operation.phase]}</span></div>}
      {visibleError && <p role="alert" className="text-red-300 text-sm">{visibleError}</p>}
      {operation?.phase === 'error' && operation.error?.retryable && <Button onClick={retry} disabled={pending} variant="outline" className="bg-slate-800 text-white border-slate-600 hover:bg-slate-700 hover:text-white"><RefreshCw className="w-4 h-4 mr-2" />Повторить подключение</Button>}
      {active && <Button onClick={back} disabled={pending} variant="outline" className="bg-slate-800 text-white border-slate-600 hover:bg-slate-700 hover:text-white">Отменить приглашение</Button>}
      <p className="flex gap-2 text-sm text-slate-400"><ShieldCheck className="w-4 h-4 flex-shrink-0 mt-0.5" />Перед началом переписки сравните код сверки на обоих устройствах. Открытие приложения по клику на ссылку появится отдельно.</p>
    </main>
  </div>;
}
