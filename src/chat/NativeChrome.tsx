import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Info } from 'lucide-react';
import { ThemeControl } from './theme';
import { HelpDialog } from './ui/HelpDialog';
import type { CaptureStatus } from './types';
import './native-chrome.css';
export function NativeChrome() {
  const [open, setOpen] = useState(false);
  const [capture, setCapture] = useState<CaptureStatus>({ platform: 'desktop', enabled: false, limitation: 'Поддержка зависит от операционной системы.' });
  useEffect(() => { let active = true; void invoke<CaptureStatus>('get_capture_protection').then(value => { if (active && value) setCapture(value); }).catch(() => undefined); return () => { active = false; }; }, []);
  return <><div className="ssc-native-chrome"><ThemeControl /><button className="ssc-icon" aria-label="Как защищён разговор" onClick={() => setOpen(true)}><Info size={19} /></button></div>
    {open && <HelpDialog capture={capture} onClose={() => setOpen(false)} openSource={async source => { await invoke('open_help_source', { source }); }} />}</>;
}
