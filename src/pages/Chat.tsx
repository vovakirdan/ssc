import { useState, useEffect, useRef } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Card } from '@/components/ui/card';
import { Send, Shield, ArrowLeft } from 'lucide-react';
import { toast } from 'sonner';
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ChatProps {
  onBack: () => void;
}

interface Message {
  id: string;
  text: string;
  timestamp: Date;
  isOwn: boolean;
}

// Состояния подключения
type ConnectionStatus = 'connected' | 'problem' | 'recovering' | 'disconnected';

const Chat = ({ onBack }: ChatProps) => {
  const [messages, setMessages] = useState<Message[]>([]);
  const [newMessage, setNewMessage] = useState('');
  const [sending, setSending] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  // Состояние подключения
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>('connected');
  // Таймер для очистки истории после отключения
  const clearHistoryTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  // Обработчик выхода из чата с отключением
  const handleBack = async () => {
    try {
      await invoke('disconnect');
    } catch (error) {
      console.error('Ошибка при отключении:', error);
    }
    onBack();
  };

  // Функция для очистки истории сообщений
  const clearMessagesHistory = () => {
    setMessages([]);
    if (clearHistoryTimeoutRef.current) {
      clearTimeout(clearHistoryTimeoutRef.current);
      clearHistoryTimeoutRef.current = null;
    }
  };

  // Очистка таймера при размонтировании компонента
  useEffect(() => {
    return () => {
      if (clearHistoryTimeoutRef.current) {
        clearTimeout(clearHistoryTimeoutRef.current);
      }
    };
  }, []);

  // Слушаем события получения сообщений от Rust ядра
  useEffect(() => {
    const unMsg = listen("ssc-message", (event: any) => {
      const messageData = event.payload;
      setMessages(prev => [...prev, {
        id: Date.now().toString(),
        text: messageData.text || messageData, // Поддерживаем разные форматы payload
        timestamp: new Date(),
        isOwn: false
      }]);
    });
    
    // Слушаем события состояния подключения
    const unConnected = listen("ssc-connected", () => {
      setConnectionStatus('connected');
    });

    const unProblem = listen("ssc-connection-problem", () => {
      setConnectionStatus('problem');
      toast.warning('Проблемы с подключением');
    });

    const unRecovering = listen("ssc-connection-recovering", () => {
      setConnectionStatus('recovering');
      toast.info('Попытка восстановления соединения...');
    });

    const unRecovered = listen("ssc-connection-recovered", () => {
      setConnectionStatus('connected');
      toast.success('Соединение восстановлено');
    });
    
    // Слушаем событие отключения собеседника
    const unDisc = listen("ssc-disconnected", () => {
      setConnectionStatus('disconnected');
      toast.error('Собеседник отключился');
      
      // Устанавливаем таймер для очистки истории через 5 секунд
      clearHistoryTimeoutRef.current = setTimeout(() => {
        clearMessagesHistory();
        toast.info('История сообщений очищена');
      }, 5000);
    });
    
    return () => {
      unMsg.then(f => {
        if (typeof f === 'function') f();
      });
      unConnected.then(f => {
        if (typeof f === 'function') f();
      });
      unProblem.then(f => {
        if (typeof f === 'function') f();
      });
      unRecovering.then(f => {
        if (typeof f === 'function') f();
      });
      unRecovered.then(f => {
        if (typeof f === 'function') f();
      });
      unDisc.then(f => {
        if (typeof f === 'function') f();
      });
    };
  }, []);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  const handleSendMessage = async (e: React.FormEvent) => {
    e.preventDefault();
    
    if (!newMessage.trim()) return;

    // Блокируем отправку при проблемах с подключением
    if (connectionStatus !== 'connected') {
      toast.error('Нет подключения. Сообщение не отправлено.');
      return;
    }

    const messageText = newMessage.trim();
    setNewMessage('');
    setSending(true);

    // Добавляем сообщение локально
    const newMsg: Message = {
      id: Date.now().toString(),
      text: messageText,
      timestamp: new Date(),
      isOwn: true
    };

    setMessages(prev => [...prev, newMsg]);

    try {
      const success = await invoke('send_text', { msg: messageText }) as boolean;
      
      if (!success) {
        toast.error('Не удалось отправить сообщение');
        // Удаляем сообщение из списка в случае ошибки
        setMessages(prev => prev.filter(msg => msg.id !== newMsg.id));
      }
    } catch (error) {
      toast.error('Ошибка отправки сообщения');
      console.error('Error sending message:', error);
      setMessages(prev => prev.filter(msg => msg.id !== newMsg.id));
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex flex-col">
        {/* Header */}
        <div className="bg-slate-800/50 border-b border-slate-700 p-4">
        <div className="flex items-center justify-between max-w-4xl mx-auto">
          <div className="flex items-center space-x-4">
            <Button variant="ghost" size="icon" onClick={handleBack} className="text-slate-400 hover:text-white">
              <ArrowLeft className="w-5 h-5" />
            </Button>
            <div className="flex items-center space-x-2">
              <Shield className="w-6 h-6 text-emerald-500" />
              <div>
                <h1 className="text-white font-semibold">Секретный чат</h1>
                <p className="text-slate-400 text-sm">End-to-end шифрование</p>
              </div>
            </div>
          </div>
          {/* Индикатор статуса подключения */}
          <div className="flex items-center space-x-2">
            <div className={`w-3 h-3 rounded-full ${
              connectionStatus === 'connected' ? 'bg-emerald-500 animate-pulse' :
              connectionStatus === 'problem' ? 'bg-yellow-500 animate-pulse' :
              connectionStatus === 'recovering' ? 'bg-blue-500 animate-pulse' :
              'bg-red-500'
            }`}></div>
            <span className={`text-sm ${
              connectionStatus === 'connected' ? 'text-emerald-400' :
              connectionStatus === 'problem' ? 'text-yellow-400' :
              connectionStatus === 'recovering' ? 'text-blue-400' :
              'text-red-400'
            }`}>
              {connectionStatus === 'connected' ? 'Подключено' :
               connectionStatus === 'problem' ? 'Проблемы' :
               connectionStatus === 'recovering' ? 'Восстановление' :
               'Отключено'}
            </span>
          </div>
        </div>
      </div>

      {/* Баннеры состояния подключения */}
      {connectionStatus === 'problem' && (
        <div className="bg-yellow-500/80 text-white text-center py-2 font-semibold">
          Проблемы с подключением
        </div>
      )}
      
      {connectionStatus === 'recovering' && (
        <div className="bg-blue-500/80 text-white text-center py-2 font-semibold">
          Попытка восстановления соединения...
        </div>
      )}
      
      {connectionStatus === 'disconnected' && (
        <div className="bg-red-500/80 text-white text-center py-2 font-semibold">
          Собеседник отключился
        </div>
      )}

      {/* Messages */}
      <div className="flex-1 overflow-y-auto p-4">
        <div className="max-w-4xl mx-auto space-y-4">
          {messages.length === 0 ? (
            <div className="text-center py-12">
              <Shield className="w-12 h-12 text-slate-600 mx-auto mb-4" />
              <p className="text-slate-400">Соединение установлено! Начните общение.</p>
              <p className="text-slate-500 text-sm mt-2">Все сообщения зашифрованы end-to-end</p>
            </div>
          ) : (
            messages.map((message) => (
              <div
                key={message.id}
                className={`flex ${message.isOwn ? 'justify-end' : 'justify-start'}`}
              >
                <Card className={`max-w-[70%] p-3 ${
                  message.isOwn 
                    ? 'bg-emerald-600 text-white' 
                    : 'bg-slate-700 text-white border-slate-600'
                }`}>
                  <p className="text-sm">{message.text}</p>
                  <p className={`text-xs mt-1 ${
                    message.isOwn ? 'text-emerald-100' : 'text-slate-400'
                  }`}>
                    {message.timestamp.toLocaleTimeString([], { 
                      hour: '2-digit', 
                      minute: '2-digit' 
                    })}
                  </p>
                </Card>
              </div>
            ))
          )}
          <div ref={messagesEndRef} />
        </div>
      </div>

      {/* Message Input */}
      <div className="bg-slate-800/50 border-t border-slate-700 p-4">
        <form onSubmit={handleSendMessage} className="max-w-4xl mx-auto">
          <div className="flex space-x-2">
            <Input
              value={newMessage}
              onChange={(e) => setNewMessage(e.target.value)}
              placeholder={
                connectionStatus === 'connected' ? "Введите сообщение..." :
                connectionStatus === 'problem' ? "Проблемы с подключением" :
                connectionStatus === 'recovering' ? "Восстановление соединения..." :
                "Соединение разорвано"
              }
              disabled={sending || connectionStatus !== 'connected'}
              className="flex-1 bg-slate-700 border-slate-600 text-white placeholder-slate-400 focus:border-emerald-500"
            />
            <Button 
              type="submit" 
              disabled={sending || !newMessage.trim() || connectionStatus !== 'connected'}
              className="bg-emerald-600 hover:bg-emerald-700"
            >
              <Send className="w-4 h-4" />
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
};

export default Chat;