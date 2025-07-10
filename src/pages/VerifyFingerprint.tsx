import { useState, useEffect } from "react";
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Shield, ArrowLeft, Loader2 } from 'lucide-react';
import { invoke } from "@tauri-apps/api/core";
import DecryptedText from "@/components/text/DecryptedText";

interface VerifyFingerprintProps {
  onConfirm: () => void;
  onCancel: () => void;
}

const VerifyFingerprint = ({ onConfirm, onCancel }: VerifyFingerprintProps) => {
  const [fingerprint, setFingerprint] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [checked, setChecked] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const getFingerprint = async () => {
      try {
        setIsLoading(true);
        setError(null);
        
        // Сначала проверяем соединение
        const connected = await invoke<boolean>("is_connected");
        
        if (!connected) {
          setError("Соединение не установлено");
          return;
        }
        
        // Получаем отпечаток
        const fp = await invoke<string>("get_fingerprint");
        
        if (fp && fp.trim() !== "") {
          setFingerprint(fp);
        } else {
          setError("Не удалось получить отпечаток");
        }
      } catch (error) {
        console.error("Ошибка получения отпечатка:", error);
        setError("Ошибка получения отпечатка");
      } finally {
        setIsLoading(false);
      }
    };

    getFingerprint();
  }, []);

  const handleConfirm = () => {
    if (fingerprint && checked) {
      onConfirm();
    }
  };

  const handleCancel = async () => {
    try {
      await invoke('disconnect');
    } catch (error) {
      console.error('Ошибка отключения:', error);
    }
    onCancel();
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex items-center justify-center p-4">
      <div className="max-w-md w-full space-y-6">
        {/* Header */}
        <div className="text-center">
          <div className="mx-auto w-20 h-20 bg-gradient-to-r from-emerald-500 to-cyan-500 rounded-full flex items-center justify-center mb-4">
            <Shield className="w-10 h-10 text-white" />
          </div>
          <h1 className="text-2xl font-bold text-white mb-2">Сверка отпечатков</h1>
          <p className="text-slate-300">
            Убедитесь, что отпечатки совпадают на обоих устройствах
          </p>
        </div>

        {/* Fingerprint Card */}
        <Card className="bg-slate-800/50 border-slate-700">
          <CardContent className="p-6">
            {isLoading ? (
              <div className="text-center">
                <Loader2 className="w-8 h-8 text-emerald-500 animate-spin mx-auto mb-4" />
                <p className="text-white font-semibold">Получение отпечатка...</p>
                <p className="text-slate-400 text-sm mt-2">Подождите, идет подключение к собеседнику</p>
              </div>
            ) : error ? (
              <div className="text-center">
                <div className="w-12 h-12 bg-red-500/20 rounded-full flex items-center justify-center mx-auto mb-4">
                  <Shield className="w-6 h-6 text-red-500" />
                </div>
                <p className="text-red-400 font-semibold">{error}</p>
                <p className="text-slate-400 text-sm mt-2">Попробуйте подключиться снова</p>
              </div>
            ) : (
              <div className="text-center">
                <div className="bg-slate-700/50 rounded-lg p-4 mb-4">
                  <p className="text-white font-mono text-lg font-bold tracking-wider">
                    <DecryptedText
                      text={fingerprint}
                      animateOn="view"
                      speed={100}
                      maxIterations={10}
                      sequential={true}
                    />
                  </p>
                </div>
                <p className="text-slate-300 text-sm">
                  Оба устройства должны показывать одинаковый код.
                  Только при совпадении кодов можно безопасно общаться.
                </p>
              </div>
            )}
          </CardContent>
        </Card>

        {/* Verification */}
        {fingerprint && !isLoading && !error && (
          <Card className="bg-slate-800/50 border-slate-700">
            <CardContent className="p-4">
              <label className="flex items-center space-x-3 cursor-pointer">
                <input
                  type="checkbox"
                  checked={checked}
                  onChange={(e) => setChecked(e.target.checked)}
                  className="w-4 h-4 text-emerald-500 bg-slate-700 border-slate-600 rounded focus:ring-emerald-500"
                />
                <span className="text-white text-sm">
                  Я вижу одинаковый код на обоих устройствах
                </span>
              </label>
            </CardContent>
          </Card>
        )}

        {/* Actions */}
        <div className="space-y-3">
          {fingerprint && !isLoading && !error && (
            <Button
              onClick={handleConfirm}
              disabled={!checked}
              className="w-full bg-gradient-to-r from-emerald-500 to-cyan-500 hover:from-emerald-600 hover:to-cyan-600 text-white font-semibold py-3 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              Коды совпадают - продолжить
            </Button>
          )}
          
          <Button
            onClick={handleCancel}
            variant="outline"
            className="w-full bg-slate-800/50 border-slate-700 text-white hover:bg-slate-700/50"
          >
            <ArrowLeft className="w-4 h-4 mr-2" />
            {error ? 'Попробовать снова' : 'Коды не совпадают'}
          </Button>
        </div>

        <p className="text-center text-slate-500 text-xs">
          Сверка отпечатков гарантирует безопасность вашего соединения
        </p>
      </div>
    </div>
  );
};

export default VerifyFingerprint;