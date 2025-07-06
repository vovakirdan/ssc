import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Scan, ArrowLeft, Link, Copy, Check } from 'lucide-react';
import { toast } from 'sonner';
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ScanQRProps {
  onBack: () => void;
  onConnected: () => void;
}

const ScanQR = ({ onBack, onConnected }: ScanQRProps) => {
  const [offerInput, setOfferInput] = useState('');
  const [answer, setAnswer] = useState('');
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);

  // Слушаем событие успешного подключения
  useEffect(() => {
    const un = listen("ssc-connected", () => onConnected());
    return () => { un.then(f => f()); };
  }, [onConnected]);

  const handleAcceptOffer = async () => {
    if (!offerInput.trim()) {
      toast.error('Введите ссылку или данные QR-кода');
      return;
    }

    setLoading(true);
    try {
      const result = await invoke('accept_offer_and_create_answer', { encoded: offerInput }) as string;
      setAnswer(result);
      toast.success('Ответ сгенерирован! Отправьте его собеседнику.');
    } catch (error) {
      toast.error('Ошибка при обработке предложения');
      console.error('Error accepting offer:', error);
    } finally {
      setLoading(false);
    }
  };

  const copyAnswer = async () => {
    try {
      await navigator.clipboard.writeText(answer);
      setCopied(true);
      toast.success('Ответ скопирован!');
      setTimeout(() => setCopied(false), 2000);
    } catch (error) {
      toast.error('Не удалось скопировать');
    }
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 p-4">
      <div className="max-w-md mx-auto space-y-6">
        <div className="flex items-center space-x-4">
          <Button variant="ghost" size="icon" onClick={onBack} className="text-slate-400 hover:text-white">
            <ArrowLeft className="w-5 h-5" />
          </Button>
          <h1 className="text-2xl font-bold text-white">Присоединиться к чату</h1>
        </div>

        <Card className="bg-slate-800/50 border-slate-700">
          <CardHeader>
            <CardTitle className="text-white flex items-center space-x-2">
              <Scan className="w-5 h-5" />
              <span>Сканирование QR-кода</span>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="w-full h-48 bg-slate-700 rounded-lg border-2 border-dashed border-slate-600 flex items-center justify-center">
              <div className="text-center">
                <Scan className="w-12 h-12 text-slate-500 mx-auto mb-2" />
                <p className="text-slate-400 text-sm">Камера для сканирования QR-кода</p>
                <p className="text-slate-500 text-xs">(функция будет реализована позже)</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card className="bg-slate-800/50 border-slate-700">
          <CardHeader>
            <CardTitle className="text-white flex items-center space-x-2">
              <Link className="w-5 h-5" />
              <span>Или введите ссылку</span>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <Textarea
              placeholder="Вставьте ссылку приглашения или данные QR-кода..."
              value={offerInput}
              onChange={(e) => setOfferInput(e.target.value)}
              className="bg-slate-700 border-slate-600 text-white min-h-[100px]"
            />
            <Button 
              onClick={handleAcceptOffer}
              disabled={loading || !offerInput.trim()}
              className="w-full bg-emerald-600 hover:bg-emerald-700"
            >
              {loading ? 'Обработка...' : 'Принять приглашение'}
            </Button>
          </CardContent>
        </Card>

        {answer && (
          <Card className="bg-slate-800/50 border-slate-700">
            <CardHeader>
              <CardTitle className="text-white">Ваш ответ</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-slate-300 text-sm">
                Скопируйте этот ответ и отправьте его собеседнику:
              </p>
              <div className="bg-slate-700 p-3 rounded border border-slate-600">
                <p className="text-white text-sm font-mono break-all">{answer}</p>
              </div>
              <Button 
                onClick={copyAnswer}
                className="w-full bg-cyan-600 hover:bg-cyan-700"
              >
                {copied ? <Check className="w-4 h-4 mr-2" /> : <Copy className="w-4 h-4 mr-2" />}
                {copied ? 'Скопировано!' : 'Скопировать ответ'}
              </Button>
              <p className="text-slate-400 text-xs text-center">
                После отправки ответа соединение будет установлено автоматически
              </p>
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
};

export default ScanQR;