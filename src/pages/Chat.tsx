import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ChatPanel } from '../chat/ui/ChatPanel';
import { useNativeChat } from '../chat/native';
export default function Chat({ onBack }: { onBack: () => void }) {
  const { model, state, capture, session, error, exit, authorizeVoice, remainingSeconds } = useNativeChat();
  const [exitError, setExitError] = useState<string | null>(null);
  const leave = async () => { try { await exit(); onBack(); } catch { setExitError('Не удалось завершить соединение. Повторите выход.'); } };
  return <div className="ssc-native-chat"><ChatPanel key={session.generation} model={model} state={state} capture={capture} ready={session.phase === 'connected'} remainingSeconds={remainingSeconds} onBeforeVoiceCapture={authorizeVoice} onOpenHelpSource={async source => { await invoke('open_help_source', { source }); }} onExit={() => { void leave(); }}>
    {(error || exitError) && <p className="ssc-notice" role="alert">{exitError || error}</p>}
  </ChatPanel></div>;
}
