import { useEffect, useRef, useState, type FormEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Send, Shield } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { MessageBubble } from '@/components/chat/MessageBubble';
import type { Message } from '@/components/chat/types';
import RoomExpiry from '@/components/RoomExpiry';
import { errorMessage, nativeListener, phaseLabels, useSession } from '@/lib/session';

export default function Chat({ onBack }: { onBack: () => void }) {
  const { session, error: sessionError } = useSession();
  const [messages, setMessages] = useState<Message[]>([]);
  const [text, setText] = useState('');
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(session.generation);
  const phase = useRef(session.phase);
  const end = useRef<HTMLDivElement>(null);
  const mounted = useRef(true);
  const sendInProgress = useRef(false);

  useEffect(() => {
    if (generation.current !== session.generation) setMessages([]);
    generation.current = session.generation;
    phase.current = session.phase;
    if (session.phase === 'closed') { setMessages([]); setText(''); }
  }, [session.generation, session.phase]);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  useEffect(() => {
    if (!session.generation) return;
    let disposed = false;
    let draining = false;
    const isClosed = () => phase.current === 'closed';
    async function drain() {
      if (draining || disposed || isClosed()) return;
      draining = true;
      try {
        const pending = await invoke<{ id: string; generation: number; text: string }[]>('drain_messages', { generation: session.generation });
        if (!disposed && !isClosed()) {
          const current = pending.filter(message => message.generation === session.generation);
          setMessages(previous => {
            const ids = new Set(previous.map(message => message.id));
            return [...previous, ...current.filter(message => !ids.has(message.id)).map(message => ({
              id: message.id, text: message.text, timestamp: new Date(), isOwn: false,
            }))];
          });
        }
      } catch (failure) { if (!disposed) setError(errorMessage(failure, 'Не удалось получить входящие сообщения')); }
      finally { draining = false; }
    }
    const stop = nativeListener('ssc-chat-message', () => { void drain(); }, undefined, () => { void drain(); });
    const timer = setInterval(() => { void drain(); }, 200);
    return () => { disposed = true; stop(); clearInterval(timer); };
  }, [session.generation]);
  useEffect(() => { end.current?.scrollIntoView?.({ behavior: 'smooth' }); }, [messages]);

  async function send(event: FormEvent) {
    event.preventDefault();
    if (sendInProgress.current || session.phase !== 'connected' || !text.trim()) return;
    if (new TextEncoder().encode(text).length > 256 * 1024) { setError('Сообщение превышает 256 KiB'); return; }
    sendInProgress.current = true;
    setSending(true);
    setError(null);
    const id = crypto.randomUUID();
    const submittedText = text;
    const submittedGeneration = session.generation;
    setMessages(previous => [...previous, { id, text: submittedText, timestamp: new Date(), isOwn: true, status: 'pending' }]);
    try {
      const accepted = await invoke<boolean>('send_text', { text: submittedText });
      if (!accepted) throw new Error('Не удалось передать сообщение каналу');
      if (mounted.current && generation.current === submittedGeneration) {
        setMessages(previous => previous.map(message => message.id === id ? { ...message, status: 'sent' } : message));
        setText('');
      }
    } catch (failure) {
      if (mounted.current && generation.current === submittedGeneration) {
        setMessages(previous => previous.map(message => message.id === id ? { ...message, status: 'failed' } : message));
        setError(errorMessage(failure, 'Не удалось отправить сообщение. Текст сохранён; часть сообщения могла быть передана.'));
      }
    } finally {
      sendInProgress.current = false;
      if (mounted.current) setSending(false);
    }
  }
  async function back() {
    try { await invoke('disconnect'); onBack(); }
    catch (failure) { setError(errorMessage(failure)); }
  }
  return <div className="h-screen flex flex-col bg-slate-900 text-white">
    <header className="p-4 border-b border-slate-700 flex gap-3 items-center">
      <Button variant="ghost" size="icon" aria-label="Выйти из чата" onClick={back}><ArrowLeft /></Button>
      <Shield className="text-emerald-500" /><h1>Секретный чат</h1>
      <span role="status" className="ml-auto text-sm">{phaseLabels[session.phase]}</span>
    </header>
    {session.phase !== 'closed' && <RoomExpiry generation={session.generation} />}
    {(error || sessionError) && <p role="alert" className="p-3 bg-red-950">{error || sessionError}</p>}
    <main className="flex-1 overflow-y-auto p-4"><div className="max-w-4xl mx-auto space-y-2">
      {messages.length === 0 && <p className="text-slate-400 text-center py-8">{session.phase === 'connected' ? 'Соединение установлено. Начните общение.' : phaseLabels[session.phase]}</p>}
      {messages.map(message => <MessageBubble key={message.id} msg={message} />)}<div ref={end} />
    </div></main>
    <form onSubmit={send} className="flex gap-2 p-4 border-t border-slate-700">
      <Input aria-label="Сообщение" value={text} onChange={event => setText(event.target.value)} disabled={sending || session.phase !== 'connected'} placeholder="Введите сообщение…" autoResize maxRows={3} className="bg-slate-800 text-white" />
      <Button aria-label="Отправить" disabled={sending || !text.trim() || session.phase !== 'connected'} type="submit"><Send /></Button>
    </form>
  </div>;
}
