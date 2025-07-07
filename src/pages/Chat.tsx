import { useState, useEffect, useRef } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Card } from '@/components/ui/card';
import { Send, Shield, ArrowLeft } from 'lucide-react';
import { toast } from 'sonner';
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import FingerprintModal from '@/components/FingerprintModal';

interface ChatProps {
  onBack: () => void;
}

interface Message {
  id: string;
  text: string;
  timestamp: Date;
  isOwn: boolean;
}

const Chat = ({ onBack }: ChatProps) => {
  const [messages, setMessages] = useState<Message[]>([]);
  const [newMessage, setNewMessage] = useState('');
  const [sending, setSending] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  // Состояние для отслеживания отключения собеседника
  const [disconnected, setDisconnected] = useState(false);
  // Состояние для подтверждения fingerprint'а
  const [verified, setVerified] = useState(false);

  // Функция для обработки отмены fingerprint'а
  const handleFingerprintCancel = async () => {
    try {
      await invoke('disconnect');
    } catch (error) {
      console.error('Error disconnecting:', error);
    }
    onBack();
  };

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
    // Слушаем событие отключения собеседника
    const unDisc = listen("ssc-disconnected", () => {
      setDisconnected(true);
      // Показываем уведомление
      toast.error('Собеседник отключился');
    });
    return () => {
      unMsg.then(f => f());
      unDisc.then(f => f());
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
    
    if (!verified) return; // блок до проверки fingerprint'а
    if (!newMessage.trim()) return;

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
    <>
      {!verified && <FingerprintModal 
        onConfirm={() => setVerified(true)} 
        onCancel={handleFingerprintCancel}
      />}
      
      <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex flex-col" 
           style={{opacity: verified ? 1 : 0.3, pointerEvents: verified ? "auto" : "none"}}>
        {/* Header */}
        <div className="bg-slate-800/50 border-b border-slate-700 p-4">
        <div className="flex items-center justify-between max-w-4xl mx-auto">
          <div className="flex items-center space-x-4">
            <Button variant="ghost" size="icon" onClick={onBack} className="text-slate-400 hover:text-white">
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
          <div className={`w-3 h-3 rounded-full ${disconnected ? 'bg-red-500' : 'bg-emerald-500 animate-pulse'}`}></div>
        </div>
      </div>

      {/* Баннер об отключении собеседника */}
      {disconnected && (
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
              placeholder="Введите сообщение..."
              disabled={sending}
              className="flex-1 bg-slate-700 border-slate-600 text-white placeholder-slate-400 focus:border-emerald-500"
            />
            <Button 
              type="submit" 
              disabled={sending || !newMessage.trim()}
              className="bg-emerald-600 hover:bg-emerald-700"
            >
              <Send className="w-4 h-4" />
            </Button>
          </div>
        </form>
      </div>
    </div>
    </>
  );
};

export default Chat;