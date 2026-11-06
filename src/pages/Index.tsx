import { useState, useEffect } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { errorMessage, nativeListener, useSession, type VerificationCode } from "@/lib/session";
import { applySavedIceServers } from "@/lib/preferences";
import DiagnosticsPanel from "@/components/DiagnosticsPanel";
import Welcome from './Welcome';
import GenerateQR from './GenerateQR';
import ScanQR from './ScanQR';
import Chat from './Chat';
import VerifyFingerprint from './VerifyFingerprint';
import ConnectionHub from './ConnectionHub';
import Settings from './Settings';
import LinkConnection from './LinkConnection';

type AppMode = 'welcome' | 'generate' | 'scan' | 'verify' | 'chat' | 'settings' | 'link';

const IndexContent = () => {
  const { session, current: currentSession, error: connectionError } = useSession();
  useEffect(() => { if (connectionError) toast.error(connectionError); }, [connectionError]);
  const [mode, setMode] = useState<AppMode>('welcome');
  const [showOptions, setShowOptions] = useState(false);
  const [ttl, setTtl] = useState(5); // TTL по умолчанию 5 минут
  const [pendingSas, setPendingSas] = useState<VerificationCode | null>(null);

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
    const unlistenSas = nativeListener<VerificationCode>('ssc-sas', (event) => {
      const { generation } = currentSession.current;
      if (event.payload.generation < generation ||
        (event.payload.generation === generation && ['idle', 'closed'].includes(currentSession.current.phase))) return;
      setPendingSas(previous => !previous || event.payload.generation >= previous.generation ? event.payload : previous);
      setMode('verify');
    });

    return () => {
      unlistenSas();
    };
  }, [currentSession]);

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
      session={session}
      sessionError={connectionError}
      onConfirm={handleVerifyConfirm} 
      onCancel={handleVerifyCancel}
      initialSas={pendingSas}
    />;
  }

  if (mode === 'generate') {
    return <GenerateQR onBack={handleBack} onConnected={handleConnected} autoGenerate={true} ttl={ttl} />;
  }

  if (mode === 'link') {
    return <LinkConnection ttl={ttl} onBack={() => { setMode('welcome'); setShowOptions(false); loadTTL(); }} />;
  }

  if (mode === 'scan') {
    return <ScanQR onBack={handleBack} onConnected={handleConnected} />;
  }

  if (showOptions) {
    return <ConnectionHub onCreate={handleGenerate} onJoin={() => { if (preferencesReady) setMode('scan'); }} onBack={handleBack} onLink={() => { if (preferencesReady) { loadTTL(); setMode('link'); } }} />;
  }

  return <>{preferencesError && <p role="alert" className="fixed top-0 left-0 right-0 bg-red-950 text-white p-3 z-40">{preferencesError}</p>}<Welcome onStart={handleStart} onSettings={handleSettings} /></>;
};

export default function Index() { return <><IndexContent /><DiagnosticsPanel /></>; }
