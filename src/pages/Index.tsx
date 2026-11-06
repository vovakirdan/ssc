import { useState, useEffect } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { errorMessage, nativeListener, useSession } from "@/lib/session";
import { applySavedIceServers } from "@/lib/preferences";
import DiagnosticsPanel from "@/components/DiagnosticsPanel";
import Welcome from './Welcome';
import GenerateQR from './GenerateQR';
import ScanQR from './ScanQR';
import Chat from './Chat';
import VerifyFingerprint from './VerifyFingerprint';
import GradientText from '@/components/text/GradientText';
import Settings from './Settings';

type AppMode = 'welcome' | 'generate' | 'scan' | 'verify' | 'chat' | 'settings';

const IndexContent = () => {
  const { error: connectionError } = useSession();
  useEffect(() => { if (connectionError) toast.error(connectionError); }, [connectionError]);
  const [mode, setMode] = useState<AppMode>('welcome');
  const [showOptions, setShowOptions] = useState(false);
  const [ttl, setTtl] = useState(5); // TTL по умолчанию 5 минут
  const [pendingSas, setPendingSas] = useState<string | null>(null); // Сохраняем SAS для передачи в VerifyFingerprint

  const [preferencesReady, setPreferencesReady] = useState(false);
  const [preferencesError, setPreferencesError] = useState<string | null>(null);
  useEffect(() => {
    if (mode !== 'welcome') return;
    let disposed = false;
    setPreferencesReady(false);
    void applySavedIceServers().then(() => {
      if (!disposed) { setPreferencesReady(true); setPreferencesError(null); }
    }).catch(failure => {
      if (!disposed) setPreferencesError(errorMessage(failure, 'Не удалось применить сохранённые серверы. Проверьте настройки.'));
    });
    return () => { disposed = true; };
  }, [mode]);

  // Загружаем TTL из настроек при монтировании
  useEffect(() => {
    const savedSettings = localStorage.getItem('ssc-settings');
    if (savedSettings) {
      try {
        const parsedSettings = JSON.parse(savedSettings);
        if (typeof parsedSettings.offerTTL === 'number') {
          setTtl(parsedSettings.offerTTL);
        }
      } catch {
          // Invalid stored preferences are ignored; defaults remain active.
      }
    }
  }, []);

  // Слушаем событие ssc-sas для автоматического перехода к проверке SAS
  useEffect(() => {
    const unlistenSas = nativeListener<string>('ssc-sas', (event) => {

      setPendingSas(event.payload); // Сохраняем SAS
      setMode('verify');
    });

    return () => {
      unlistenSas();
    };
  }, []);

  const handleStart = () => {
    if (!preferencesReady) { toast.error(preferencesError || 'Применяем настройки подключения…'); return; }
    setShowOptions(true);
  };

  const handleBack = async () => {
    if (["generate", "scan", "verify"].includes(mode)) {
      try { await invoke("disconnect"); }
      catch (failure) { toast.error(errorMessage(failure)); return; }
    }
    if (showOptions) {
      setShowOptions(false);
      setMode('welcome');
    } else {
      setMode('welcome');
    }
    // Обновляем TTL при возврате (на случай если пользователь изменил настройки)
    loadTTL();
  };

  const handleSettings = () => {
    setMode('settings');
  };

  // Функция для загрузки TTL из настроек
  const loadTTL = () => {
    const savedSettings = localStorage.getItem('ssc-settings');
    if (savedSettings) {
      try {
        const parsedSettings = JSON.parse(savedSettings);
        if (typeof parsedSettings.offerTTL === 'number') {
          setTtl(parsedSettings.offerTTL);
        }
      } catch {
          // Invalid stored preferences are ignored; defaults remain active.
      }
    }
  };

  // Обновляем TTL при переходе в режим generate
  const handleGenerate = () => {
    if (!preferencesReady) return;
    loadTTL(); // Загружаем актуальный TTL
    setMode('generate');
  };

  const handleConnected = () => {

    setMode('verify');
  };

  // Очищаем pendingSas при переходе к чату
  const handleVerifyConfirm = () => {
    setPendingSas(null);
    setMode('chat');
  };

  // Очищаем pendingSas при отмене
  const handleVerifyCancel = () => {
    setPendingSas(null);
    setMode('welcome');
  };

  if (mode === 'settings') {
    return <Settings onBack={handleBack} />;
  }

  if (mode === 'chat') {
    return <Chat onBack={() => setMode('welcome')} />;
  }

  if (mode === 'verify') {
    return <VerifyFingerprint 
      onConfirm={handleVerifyConfirm} 
      onCancel={handleVerifyCancel}
      initialSas={pendingSas} // Передаем сохраненный SAS
    />;
  }

  if (mode === 'generate') {
    return <GenerateQR onBack={handleBack} onConnected={handleConnected} autoGenerate={true} ttl={ttl} />;
  }

  if (mode === 'scan') {
    return <ScanQR onBack={handleBack} onConnected={handleConnected} />;
  }

  if (showOptions) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex items-center justify-center p-4">
        <div className="max-w-md w-full space-y-6">
          <div className="text-center">
            <h2 className="text-3xl font-bold text-white mb-4">
              <GradientText
                colors={['#40ffaa', '#4079ff', '#40ffaa', '#4079ff', '#40ffaa']}
                animationSpeed={5}
              >
                Выберите действие
              </GradientText>
            </h2>
            <p className="text-slate-300">Создайте новое подключение или присоединитесь к существующему</p>
          </div>

          <div className="space-y-4">
            <button
              onClick={handleGenerate}
              className="w-full p-6 bg-slate-800/50 hover:bg-slate-700/50 border border-slate-700 rounded-lg transition-all text-left"
            >
              <h3 className="text-white font-semibold mb-2">Создать QR-код</h3>
              <p className="text-slate-400 text-sm">Создайте новое подключение и поделитесь QR-кодом</p>
            </button>

            <button
              onClick={() => { if (preferencesReady) setMode('scan'); }}
              className="w-full p-6 bg-slate-800/50 hover:bg-slate-700/50 border border-slate-700 rounded-lg transition-all text-left"
            >
              <h3 className="text-white font-semibold mb-2">Сканировать QR-код</h3>
              <p className="text-slate-400 text-sm">Присоединитесь к существующему подключению</p>
            </button>
          </div>

          <button
            onClick={handleBack}
            className="w-full text-slate-400 hover:text-white transition-colors"
          >
            ← Назад
          </button>
        </div>
      </div>
    );
  }

  return <>{preferencesError && <p role="alert" className="fixed top-0 left-0 right-0 bg-red-950 text-white p-3 z-40">{preferencesError}</p>}<Welcome onStart={handleStart} onSettings={handleSettings} /></>;
};

export default function Index() { return <><IndexContent /><DiagnosticsPanel /></>; }