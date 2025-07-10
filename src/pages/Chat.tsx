import {useState, useEffect, useRef, FormEvent} from 'react';
import {ArrowLeft, Send, Shield} from 'lucide-react';
import {Button} from '@/components/ui/button';
import {Input} from '@/components/ui/input';
import {toast} from 'sonner';
import {invoke} from '@tauri-apps/api/core';
import {listen, UnlistenFn} from '@tauri-apps/api/event';
import {useWindowSize} from 'react-use';

import {Message, ConnectionStatus} from '@/components/chat/types';
import {MessageBubble} from '@/components/chat/MessageBubble';
import ShinyText from '@/components/text/ShinyText';
import { AnimatePresence } from 'framer-motion';

interface ChatProps {
  onBack: () => void;
}

export default function Chat({onBack}: ChatProps) {
  /* ---------- state ---------- */
  const [messages, setMessages]           = useState<Message[]>([]);
  const [newMessage, setNewMessage]       = useState<string>('');
  const [sending, setSending]             = useState<boolean>(false);
  const [status, setStatus]               = useState<ConnectionStatus>('connected');
  const messagesEndRef                    = useRef<HTMLDivElement>(null);
  const clearHistoryTimeoutRef            = useRef<NodeJS.Timeout | null>(null);
  const unlistenersRef                    = useRef<UnlistenFn[]>([]);

  /* ---------- helpers ---------- */
  const scrollToBottom = () =>
    messagesEndRef.current?.scrollIntoView({behavior: 'smooth'});

  const clearHistory = () => {
    setMessages([]);
    if (clearHistoryTimeoutRef.current) clearTimeout(clearHistoryTimeoutRef.current);
  };

  /* ---------- lifecycle ---------- */
  useEffect(() => {
    // ↓ register all native listeners in one place
    const register = (event: string, cb: Parameters<typeof listen>[1]) => {
      listen(event, cb).then((un) => unlistenersRef.current.push(un));
    };

    register('ssc-message', (e) => {
      const txt = (e.payload as any).text ?? e.payload;
      setMessages((prev) => [
        ...prev,
        {id: Date.now().toString(), text: txt, timestamp: new Date(), isOwn: false},
      ]);
    });

    register('ssc-connected', () => setStatus('connected'));
    register('ssc-connection-problem', () => {
      setStatus('problem');
      toast.warning('Проблемы с подключением');
    });
    register('ssc-connection-recovering', () => {
      setStatus('recovering');
      toast.info('Попытка восстановления соединения…');
    });
    register('ssc-connection-recovered', () => {
      setStatus('connected');
      toast.success('Соединение восстановлено');
    });
    register('ssc-disconnected', () => {
      setStatus('disconnected');
      toast.error('Собеседник отключился');
      clearHistoryTimeoutRef.current = setTimeout(() => {
        clearHistory();
        toast.info('История сообщений очищена');
      }, 5_000);
    });

    return () => {
      unlistenersRef.current.forEach((un) => un());
      if (clearHistoryTimeoutRef.current) clearTimeout(clearHistoryTimeoutRef.current);
    };
  }, []);

  /* scroll down on new messages */
  useEffect(scrollToBottom, [messages]);

  /* ---------- actions ---------- */
  const handleBack = async () => {
    try {
      await invoke('disconnect');
    } catch (e) {
      console.error('disconnect error', e);
    }
    onBack();
  };

  const handleSend = async (e: FormEvent) => {
    e.preventDefault();
    if (!newMessage.trim()) return;
    if (status !== 'connected') {
      toast.error('Нет подключения. Сообщение не отправлено.');
      return;
    }

    const txt = newMessage.trim();
    setNewMessage('');
    setSending(true);

    const local: Message = {
      id: Date.now().toString(),
      text: txt,
      timestamp: new Date(),
      isOwn: true,
    };
    setMessages((p) => [...p, local]);

    try {
      const ok = await invoke<boolean>('send_text', {msg: txt});
      if (!ok) throw new Error('send_text returned false');
    } catch (err) {
      toast.error('Не удалось отправить сообщение');
      setMessages((p) => p.filter((m) => m.id !== local.id));
    } finally {
      setSending(false);
    }
  };

  /* ---------- UI ---------- */
  const {width} = useWindowSize(); // simple mobile check
  const isMobile = width < 640;

  const statusColor =
    status === 'connected'
      ? 'bg-emerald-500'
      : status === 'problem'
      ? 'bg-yellow-500'
      : status === 'recovering'
      ? 'bg-blue-500'
      : 'bg-red-500';

  const statusBgClass = status === 'connected'
    ? 'bg-emerald-500'
    : status === 'problem'
    ? 'bg-yellow-500'
    : status === 'recovering'
    ? 'bg-blue-500'
    : 'bg-red-500';

  const statusTextClass = status === 'connected'
    ? 'text-emerald-400'
    : status === 'problem'
    ? 'text-yellow-400'
    : status === 'recovering'
    ? 'text-blue-400'
    : 'text-red-400';


  return (
    <div className="min-h-screen flex flex-col bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900">
      {/* Header */}
      <header className="bg-slate-800/50 border-b border-slate-700 p-3">
        <div className="flex items-center justify-between max-w-4xl mx-auto">
          <div className="flex items-center space-x-3">
            <Button variant="ghost" size="icon" onClick={handleBack}>
              <ArrowLeft className="w-5 h-5 text-slate-400 hover:text-white" />
            </Button>
            <Shield className="w-6 h-6 text-emerald-500" />
            {!isMobile && (
              <div>
                <h1 className="text-white font-semibold">Секретный чат</h1>
                <p className="text-slate-400 text-sm">End-to-end encrypted</p>
              </div>
            )}
          </div>

          {/* Connection indicator */}
          <div className="flex items-center space-x-2">
            <div className={`w-3 h-3 rounded-full ${statusBgClass} animate-pulse`} />
            <span className={`text-sm ${statusTextClass}`}>
              <ShinyText
                text={status === 'connected'
                  ? 'Подключено'
                  : status === 'problem'
                  ? 'Проблемы'
                  : status === 'recovering'
                  ? 'Восстановление'
                  : 'Отключено'}
              />
            </span>
          </div>
        </div>
      </header>

      {/* Status banners */}
      {['problem', 'recovering', 'disconnected'].includes(status) && (
        <div
          className={`${statusColor} text-white text-center py-2 font-semibold`}
        >
          {status === 'problem'
            ? 'Проблемы с подключением'
            : status === 'recovering'
            ? 'Попытка восстановления соединения…'
            : 'Собеседник отключился'}
        </div>
      )}

      {/* Messages */}
      <main className="flex-1 overflow-y-auto p-4">
        <div className="max-w-4xl mx-auto space-y-4">
          {messages.length === 0 ? (
            <div className="text-center py-12">
              <Shield className="w-12 h-12 text-slate-600 mx-auto mb-4" />
              <p className="text-slate-400">
                Соединение установлено! Начните общение.
              </p>
              <p className="text-slate-500 text-sm mt-2">
                Все сообщения зашифрованы end-to-end
              </p>
            </div>
          ) : (
            <AnimatePresence>
              {messages.map((m) => <MessageBubble key={m.id} msg={m} />)}
            </AnimatePresence>
          )}
          <div ref={messagesEndRef} />
        </div>
      </main>

      {/* Input */}
      <form
        onSubmit={handleSend}
        className="bg-slate-800/50 border-t border-slate-700 p-3"
      >
        <div className="flex max-w-4xl mx-auto space-x-2">
          <Input
            value={newMessage}
            onChange={(e) => setNewMessage(e.target.value)}
            placeholder={
              status === 'connected'
                ? 'Введите сообщение…'
                : status === 'problem'
                ? 'Проблемы с подключением'
                : status === 'recovering'
                ? 'Восстановление соединения…'
                : 'Соединение разорвано'
            }
            disabled={sending || status !== 'connected'}
            className="flex-1 bg-slate-700 border-slate-600 text-white placeholder-slate-400 focus:border-emerald-500"
          />
          <Button
            type="submit"
            disabled={sending || !newMessage.trim() || status !== 'connected'}
            className="bg-emerald-600 hover:bg-emerald-700"
          >
            <Send className="w-4 h-4" />
          </Button>
        </div>
      </form>
    </div>
  );
}
