import { useState, useEffect } from 'react';
import Welcome from './Welcome';
import GenerateQR from './GenerateQR';
import ScanQR from './ScanQR';
import Chat from './Chat';
import VerifyFingerprint from './VerifyFingerprint';
import GradientText from '@/components/text/GradientText';
import Settings from './Settings';

type AppMode = 'welcome' | 'generate' | 'scan' | 'verify' | 'chat' | 'settings';

const Index = () => {
  const [mode, setMode] = useState<AppMode>('welcome');
  const [showOptions, setShowOptions] = useState(false);
  const [ttl, setTtl] = useState(5); // TTL по умолчанию 5 минут

  // Загружаем TTL из настроек при монтировании
  useEffect(() => {
    const savedSettings = localStorage.getItem('ssc-settings');
    if (savedSettings) {
      try {
        const parsedSettings = JSON.parse(savedSettings);
        if (typeof parsedSettings.offerTTL === 'number') {
          setTtl(parsedSettings.offerTTL);
        }
      } catch (error) {
        console.error('Error parsing saved settings:', error);
      }
    }
  }, []);

  const handleStart = () => {
    setShowOptions(true);
  };

  const handleBack = () => {
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
      } catch (error) {
        console.error('Error parsing saved settings:', error);
      }
    }
  };

  // Обновляем TTL при переходе в режим generate
  const handleGenerate = () => {
    loadTTL(); // Загружаем актуальный TTL
    setMode('generate');
  };

  const handleConnected = () => {
    console.log('Index: handleConnected called - switching to verify mode');
    setMode('verify');
  };

  if (mode === 'settings') {
    return <Settings onBack={handleBack} />;
  }

  if (mode === 'chat') {
    return <Chat onBack={() => setMode('welcome')} />;
  }

  if (mode === 'verify') {
    return <VerifyFingerprint 
      onConfirm={() => setMode('chat')} 
      onCancel={() => setMode('welcome')} 
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
              onClick={() => setMode('scan')}
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

  return <Welcome onStart={handleStart} onSettings={handleSettings} />;
};

export default Index;