import { useState } from 'react';
import Welcome from './Welcome';
import GenerateQR from './GenerateQR';
import ScanQR from './ScanQR';
import Chat from './Chat';

type AppMode = 'welcome' | 'generate' | 'scan' | 'chat';

const Index = () => {
  const [mode, setMode] = useState<AppMode>('welcome');
  const [showOptions, setShowOptions] = useState(false);

  const handleStart = () => {
    setShowOptions(true);
  };

  const handleBack = () => {
    if (showOptions) {
      setShowOptions(false);
    } else {
      setMode('welcome');
    }
  };

  const handleConnected = () => {
    setMode('chat');
  };

  if (mode === 'chat') {
    return <Chat onBack={() => setMode('welcome')} />;
  }

  if (mode === 'generate') {
    return <GenerateQR onBack={handleBack} onConnected={handleConnected} autoGenerate={true} />;
  }

  if (mode === 'scan') {
    return <ScanQR onBack={handleBack} onConnected={handleConnected} />;
  }

  if (showOptions) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex items-center justify-center p-4">
        <div className="max-w-md w-full space-y-6">
          <div className="text-center">
            <h2 className="text-2xl font-bold text-white mb-4">Выберите действие</h2>
            <p className="text-slate-300">Создайте новое подключение или присоединитесь к существующему</p>
          </div>

          <div className="space-y-4">
            <button
              onClick={() => setMode('generate')}
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

  return <Welcome onStart={handleStart} />;
};

export default Index;