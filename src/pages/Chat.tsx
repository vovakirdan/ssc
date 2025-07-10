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
import { AnimatePresence, motion } from 'framer-motion';

interface ChatProps {
  onBack: () => void;
}

export default function Chat({onBack}: ChatProps) {
  /* ---------- state ---------- */
  const [messages, setMessages]               = useState<Message[]>([]);
  const [newMessage, setNewMessage]           = useState<string>('');
  const [sending, setSending]                 = useState<boolean>(false);
  const [status, setStatus]                   = useState<ConnectionStatus>('connected');
  const [finalDisconnect, setFinalDisconnect] = useState(false);
  const finalDisconnectRef                    = useRef(false);
  const messagesEndRef                        = useRef<HTMLDivElement>(null);
  const clearHistoryTimeoutRef                = useRef<NodeJS.Timeout | null>(null);
  const unlistenersRef                        = useRef<UnlistenFn[]>([]);

  /* ---------- helpers ---------- */
  const scrollToBottom = () =>
    messagesEndRef.current?.scrollIntoView({behavior: 'smooth'});

  const clearHistory = () => {
    console.log('Очистка истории сообщений');
    // Добавляем небольшую задержку для завершения анимации
    setTimeout(() => {
      setMessages([]);
    }, 100);
    if (clearHistoryTimeoutRef.current) clearTimeout(clearHistoryTimeoutRef.current);
  };

  const statusRef = useRef<ConnectionStatus>('connected');

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

    register('ssc-connected', () => {
      setStatus('connected');
      statusRef.current = 'connected';
    });
    register('ssc-connection-problem', () => {
      setStatus('problem');
      statusRef.current = 'problem';
      toast.warning('Проблемы с подключением');
    });
    register('ssc-connection-recovering', () => {
      console.log('ssc-connection-recovering: попытка восстановления');
      setStatus('recovering');
      statusRef.current = 'recovering';
      toast.info('Попытка восстановления соединения…');
      // НЕ сбрасываем таймер очистки при попытках восстановления
      // Таймер должен работать до финального отключения
    });    
    register('ssc-connection-recovered', () => {
      console.log('ssc-connection-recovered: соединение восстановлено');
      setStatus('connected');
      statusRef.current = 'connected';
      setFinalDisconnect(false); // Сбрасываем флаг финального отключения
      finalDisconnectRef.current = false;
      
      // Отменяем таймер очистки при успешном восстановлении
      if (clearHistoryTimeoutRef.current) {
        console.log('ssc-connection-recovered: отмена таймера очистки');
        clearTimeout(clearHistoryTimeoutRef.current);
        clearHistoryTimeoutRef.current = null;
      }
      
      toast.success('Соединение восстановлено');
    });
    
    // Обработчик для финального отключения после неудачных попыток восстановления
    register('ssc-connection-failed', () => {
      console.log('ssc-connection-failed: восстановление не удалось');
      setStatus('disconnected');
      statusRef.current = 'disconnected';
      toast.error('Восстановление соединения не удалось');
      
      // Если таймер очистки еще не запущен, запускаем его
      // Если уже запущен - оставляем как есть, он продолжит работать
      if (!clearHistoryTimeoutRef.current) {
        console.log('ssc-connection-failed: запуск нового таймера очистки');
        clearHistoryTimeoutRef.current = setTimeout(() => {
          console.log('ssc-connection-failed: таймер сработал, проверяем условия очистки');
          if (statusRef.current !== 'connected' && !finalDisconnectRef.current) {
            clearHistory();
            toast.info('История сообщений очищена');
            setFinalDisconnect(true);
            finalDisconnectRef.current = true;
          } else {
            console.log('ssc-connection-failed: условия очистки не выполнены', {
              status: statusRef.current,
              finalDisconnect: finalDisconnectRef.current
            });
          }
        }, 5000); // 5 секунд после финального отключения
      } else {
        console.log('ssc-connection-failed: таймер уже запущен, оставляем как есть', {
          currentStatus: statusRef.current,
          finalDisconnect: finalDisconnectRef.current
        });
      }
    });
    register('ssc-disconnected', () => {
      console.log('ssc-disconnected: запуск таймера очистки', {
        currentStatus: statusRef.current,
        finalDisconnect: finalDisconnectRef.current,
        hasTimer: !!clearHistoryTimeoutRef.current
      });
      setStatus('disconnected');
      statusRef.current = 'disconnected';
      toast.error('Собеседник отключился');
      setFinalDisconnect(false);
      finalDisconnectRef.current = false;
    
      if (clearHistoryTimeoutRef.current) {
        console.log('ssc-disconnected: отмена предыдущего таймера');
        clearTimeout(clearHistoryTimeoutRef.current);
      }
    
      clearHistoryTimeoutRef.current = setTimeout(() => {
        console.log('ssc-disconnected: таймер сработал, проверяем условия очистки');
        // Проверяем, что мы не подключены и еще не очистили историю
        if (statusRef.current !== 'connected' && !finalDisconnectRef.current) {
          clearHistory();
          toast.info('История сообщений очищена');
          setFinalDisconnect(true);
          finalDisconnectRef.current = true;
        } else {
          console.log('ssc-disconnected: условия очистки не выполнены', {
            status: statusRef.current,
            finalDisconnect: finalDisconnectRef.current,
            statusType: typeof statusRef.current
          });
        }
      }, 15000); // 15 секунд = grace period (10с) + дополнительное время (5с)
    });    

    return () => {
      unlistenersRef.current.forEach((un) => un());
      if (clearHistoryTimeoutRef.current) clearTimeout(clearHistoryTimeoutRef.current);
    };
  }, []);

  /* scroll down on new messages */
  useEffect(() => {
    // Небольшая задержка для завершения анимации появления сообщения
    const timer = setTimeout(scrollToBottom, 100);
    return () => clearTimeout(timer);
  }, [messages]);

  /* sync finalDisconnect state with ref */
  useEffect(() => {
    finalDisconnectRef.current = finalDisconnect;
  }, [finalDisconnect]);

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
    <div className="h-screen flex flex-col bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 overflow-hidden">
      {/* Header - фиксированная шапка */}
      <header className="flex-shrink-0 bg-slate-800/50 border-b border-slate-700 p-3">
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

      {/* Status banners - фиксированные баннеры */}
      {['problem', 'recovering', 'disconnected'].includes(status) && (
        <div
          className={`flex-shrink-0 ${statusColor} text-white text-center py-2 font-semibold`}
        >
          {status === 'problem'
            ? 'Проблемы с подключением'
            : status === 'recovering'
            ? 'Попытка восстановления соединения…'
            : 'Собеседник отключился'}
        </div>
      )}

      {/* Messages - скроллируемая область */}
      <main className="flex-1 overflow-y-auto p-4 min-h-0">
        <div className="max-w-4xl mx-auto space-y-2 w-full">
          <AnimatePresence mode="popLayout">
            {messages.length === 0 ? (
              <motion.div
                key="empty-state"
                initial={{ opacity: 0, scale: 0.8 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.8 }}
                transition={{ duration: 0.3 }}
                className="text-center py-12"
              >
                <Shield className="w-12 h-12 text-slate-600 mx-auto mb-4" />
                <p className="text-slate-400">
                  Соединение установлено! Начните общение.
                </p>
                <p className="text-slate-500 text-sm mt-2">
                  Все сообщения зашифрованы end-to-end
                </p>
              </motion.div>
            ) : (
              messages.map((m, index) => (
                <MessageBubble 
                  key={m.id} 
                  msg={m} 
                  index={index}
                />
              ))
            )}
          </AnimatePresence>
          <div ref={messagesEndRef} />
        </div>
      </main>

      {/* Input - фиксированное поле ввода */}
      <form
        onSubmit={handleSend}
        className="flex-shrink-0 bg-slate-800/50 border-t border-slate-700 p-3"
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
