import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage, phaseLabels, useSession } from '@/lib/session';

interface DiagnosticEvent {
  timestamp: string;
  session_id: number;
  component: string;
  stage: string;
  outcome: string;
  code: string | null;
}
interface Diagnostics { enabled: boolean; events: DiagnosticEvent[] }

export default function DiagnosticsPanel() {
  const [open, setOpen] = useState(false);
  const [report, setReport] = useState<Diagnostics>({ enabled: false, events: [] });
  const [error, setError] = useState<string | null>(null);
  const { session } = useSession();
  useEffect(() => {
    if (!open) return;
    let disposed = false;
    async function refresh() {
      try {
        const next = await invoke<Diagnostics>('get_diagnostics');
        if (!disposed) setReport(next);
      } catch (failure) { if (!disposed) setError(errorMessage(failure)); }
    }
    void refresh();
    const timer = setInterval(() => { void refresh(); }, 1000);
    return () => { disposed = true; clearInterval(timer); };
  }, [open]);
  async function toggle(enabled: boolean) {
    try {
      await invoke('set_diagnostics', { enabled });
      setReport(await invoke<Diagnostics>('get_diagnostics'));
      setError(null);
    } catch (failure) { setError(errorMessage(failure)); }
  }
  async function clear() {
    try { await invoke('clear_diagnostics'); setReport(previous => ({ ...previous, events: [] })); }
    catch (failure) { setError(errorMessage(failure)); }
  }
  async function download() {
    try {
      const snapshot = await invoke<Diagnostics>('get_diagnostics');
      const url = URL.createObjectURL(new Blob([JSON.stringify(snapshot, null, 2)], { type: 'application/json' }));
      const link = document.createElement('a');
      link.href = url; link.download = 'ssc-diagnostics.json'; link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (failure) { setError(errorMessage(failure)); }
  }
  return <div className="fixed bottom-3 right-3 z-50 text-white">
    <button className="rounded bg-slate-700 px-3 py-2 text-xs" onClick={() => setOpen(value => !value)} aria-expanded={open}>Диагностика</button>
    {open && <section aria-label="Диагностика" className="absolute bottom-12 right-0 w-[min(90vw,40rem)] max-h-[75vh] overflow-auto bg-slate-950 border border-slate-600 rounded-lg p-4 shadow-xl space-y-3">
      <h2 className="font-semibold">Диагностика подключения</h2>
      <p className="text-sm">{phaseLabels[session.phase]}{session.route && ` · ${session.route === 'relay' ? 'Через TURN' : 'Прямой маршрут'}`}</p>
      <label className="flex gap-2"><input type="checkbox" checked={report.enabled} onChange={event => { void toggle(event.target.checked); }} />Записывать события</label>
      <p className="text-xs text-slate-400">Последние 2 000 событий хранятся в памяти. Тексты переписки, ключи и адреса в отчёт не входят. Выключение очищает журнал.</p>
      <div className="flex gap-3 text-sm"><button onClick={clear}>Очистить</button><button onClick={download}>Скачать JSON</button><button onClick={() => setOpen(false)}>Закрыть</button></div>
      {error && <p role="alert">{error}</p>}
      <ol className="font-mono text-xs space-y-1" aria-label="События">
        {report.events.map((event, index) => <li key={index}>{event.timestamp} · {event.session_id} · {event.component} / {event.stage} · {event.outcome}{event.code ? ` · ${event.code}` : ''}</li>)}
      </ol>
    </section>}
  </div>;
}
