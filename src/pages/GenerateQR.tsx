import { useState, useEffect, useRef } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { QrCode as QrCodeIcon, Copy, Check, ArrowLeft, Scan, Download, X } from 'lucide-react';
import { toast } from 'sonner';
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { QRCodeSVG } from 'qrcode.react';
import { QRCodeCanvas } from 'qrcode.react';

interface GenerateQRProps {
  onBack: () => void;
  onConnected: () => void;
  autoGenerate?: boolean;
}

const GenerateQR = ({ onBack, onConnected, autoGenerate }: GenerateQRProps) => {
  const [offer, setOffer] = useState<string>('');
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);
  const [answer, setAnswer] = useState('');
  const [awaitingAnswer, setAwaitingAnswer] = useState(false);
  // TTL для QR-кода (секунды)
  const TTL = 300;
  const [ttl, setTtl] = useState(TTL);
  const timerRef = useRef<NodeJS.Timeout | null>(null);

  // Слушаем событие успешного подключения
  useEffect(() => {
    const un = listen("ssc-connected", () => onConnected());
    return () => { un.then(f => f()); };
  }, [onConnected]);

  // Генерируем QR-код автоматически при открытии страницы
  useEffect(() => {
    if (autoGenerate && !offer && !loading) {
      generateOffer();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoGenerate]);

  // Обновляем TTL и перегенерируем QR-код по истечении времени
  useEffect(() => {
    if (!offer) {
      setTtl(TTL);
      if (timerRef.current) clearInterval(timerRef.current);
      return;
    }
    setTtl(TTL);
    if (timerRef.current) clearInterval(timerRef.current);
    timerRef.current = setInterval(() => {
      setTtl(prev => {
        if (prev <= 1) {
          // Время истекло — генерируем новый QR-код
          generateOffer();
          return TTL;
        }
        return prev - 1;
      });
    }, 1000);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [offer]);

  const generateOffer = async () => {
    setLoading(true);
    try {
      const result = await invoke('generate_offer') as string;
      setOffer(result);
      setAwaitingAnswer(true);
      toast.success('QR-код сгенерирован!');
    } catch (error) {
      toast.error('Ошибка генерации QR-кода');
      console.error('Error generating offer:', error);
    } finally {
      setLoading(false);
    }
  };

  const copyToClipboard = async () => {
    try {
      await navigator.clipboard.writeText(offer);
      setCopied(true);
      toast.success('Ссылка скопирована!');
      setTimeout(() => setCopied(false), 2000);
    } catch (error) {
      toast.error('Не удалось скопировать');
    }
  };

  const handleAnswerSubmit = async () => {
    if (!answer.trim()) {
      toast.error('Введите ответ');
      return;
    }

    setLoading(true);
    try {
      const success = await invoke('set_answer', { encoded: answer }) as boolean;
      if (success) {
        toast.success('Соединение установлено!');
        setTimeout(() => onConnected(), 1000);
      } else {
        toast.error('Не удалось установить соединение');
      }
    } catch (error) {
      toast.error('Ошибка при установке соединения');
      console.error('Error setting answer:', error);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 p-4">
      <div className="max-w-md mx-auto space-y-6">
        <div className="flex items-center space-x-4">
          <Button variant="ghost" size="icon" onClick={onBack} className="text-slate-400 hover:text-white">
            <ArrowLeft className="w-5 h-5" />
          </Button>
          <h1 className="text-2xl font-bold text-white">Создать подключение</h1>
        </div>

        <Card className="bg-slate-800/50 border-slate-700">
          <CardHeader>
            <CardTitle className="text-white flex items-center space-x-2">
              <QrCodeIcon className="w-5 h-5" />
              <span>Генерация QR-кода</span>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            {!offer ? (
              <div className="w-full text-center text-slate-400 py-8">Генерация QR-кода...</div>
            ) : (
              <div className="space-y-4">
                <div className="bg-white p-4 rounded-lg">
                  {/* TTL и прогресс */}
                  <div className="flex justify-between items-center mb-2">
                    <span className="text-xs text-slate-500">QR-код истечёт через {ttl} сек.</span>
                    <div className="w-32 h-2 bg-slate-300 rounded overflow-hidden">
                      <div className="h-2 bg-emerald-500 transition-all" style={{ width: `${(ttl/TTL)*100}%` }} />
                    </div>
                  </div>
                  <div className="w-full aspect-square bg-slate-200 rounded flex items-center justify-center relative">
                    <div className="w-full h-full flex items-center justify-center">
                      <QRCodeSVG value={offer} width="100%" height="100%" style={{ width: '100%', height: '100%' }} />
                    </div>
                    <div style={{ position: 'absolute', left: '-9999px', top: 0 }}>
                      <QRCodeCanvas id="qr-canvas" value={offer} size={512} />
                    </div>
                  </div>
                </div>
                <div className="flex items-center mt-2">
                  <span className="text-slate-300 mr-2">Сохранить QR</span>
                  <Button
                    size="icon"
                    className="bg-slate-600 hover:bg-slate-500"
                    onClick={() => {
                      const canvas = document.getElementById('qr-canvas') as HTMLCanvasElement | null;
                      if (!canvas) return;
                      const url = canvas.toDataURL('image/png');
                      const a = document.createElement('a');
                      a.href = url;
                      a.download = 'qr-code.png';
                      document.body.appendChild(a);
                      a.click();
                      document.body.removeChild(a);
                      toast.success('QR-код сохранён в PNG!');
                    }}
                    title="Сохранить QR-код в PNG"
                  >
                    <Download className="w-5 h-5" />
                  </Button>
                </div>
                <div className="space-y-2">
                  <label className="text-slate-300 text-sm">Или поделитесь ссылкой:</label>
                  <div className="flex space-x-2">
                    <Input 
                      value={offer} 
                      readOnly 
                      className="bg-slate-700 border-slate-600 text-white"
                    />
                    <Button 
                      size="icon" 
                      onClick={copyToClipboard}
                      className="bg-slate-600 hover:bg-slate-500"
                    >
                      {copied ? <Check className="w-4 h-4" /> : <Copy className="w-4 h-4" />}
                    </Button>
                  </div>
                </div>
              </div>
            )}
          </CardContent>
        </Card>

        {awaitingAnswer && (
          <Card className="bg-slate-800/50 border-slate-700">
            <CardHeader>
              <CardTitle className="text-white flex items-center space-x-2">
                <Scan className="w-5 h-5" />
                <span>Ожидание ответа</span>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-slate-300 text-sm">
                Вставьте ответ от собеседника:
              </p>
              <div className="flex space-x-2">
                <Input
                  placeholder="Вставьте ответ здесь..."
                  value={answer}
                  onChange={(e) => setAnswer(e.target.value)}
                  className="bg-slate-700 border-slate-600 text-white h-10 resize-none"
                  style={{ maxWidth: '100%', overflow: 'hidden' }}
                />
                <Button
                  size="icon"
                  onClick={() => setAnswer('')}
                  disabled={!answer.trim()}
                  className="bg-red-600 hover:bg-red-500"
                  title="Очистить"
                >
                  <X className="w-4 h-4" />
                </Button>
              </div>
              <Button 
                onClick={handleAnswerSubmit}
                disabled={loading || !answer.trim()}
                className="w-full bg-cyan-600 hover:bg-cyan-700"
              >
                {loading ? 'Подключение...' : 'Подключиться'}
              </Button>
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
};

export default GenerateQR;