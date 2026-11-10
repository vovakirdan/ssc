import { invoke } from '@tauri-apps/api/core';
import { useEffect, useLayoutEffect, useMemo, useSyncExternalStore, useState, useRef } from 'react';
import { nativeListener, useSession, type SessionState } from '../lib/session';
import { ChatModel } from './model';
import type { CaptureStatus, ChatEvent, HashSink } from './types';
import { playableVoiceFormats } from './voice/recorder';
const closedNotice = (session: SessionState) => session.close_reason === 'peer_left' ? 'Собеседник вышел. История чата на этом устройстве очищена.'
  : session.close_reason === 'expired' ? 'Время чата истекло. История на этом устройстве очищена.'
    : session.close_reason === 'local_left' ? 'Вы вышли. История чата на этом устройстве очищена.' : 'Связь прервалась. История чата на этом устройстве очищена.';
export function useNativeChat() {
  const { session, current, error } = useSession();
  const generation = session.generation;
  const [capture, setCapture] = useState<CaptureStatus>({ platform: 'desktop', enabled: false, limitation: 'Возможности ОС проверяются' });
  const [remainingSeconds, setRemainingSeconds] = useState<number | null>(null);
  const model = useMemo(() => new ChatModel({
    isReady: () => current.current.generation === generation && ['connected', 'recovering'].includes(current.current.phase),
    send: event => invoke('send_chat_event', { generation, event }),
    onPeerExit: reason => { if (reason === 'invalid' && current.current.generation === generation) void invoke('disconnect', { reason: 'error', generation }).catch(() => undefined); },
    createHasher: async id => {
      await invoke('hash_attachment_chunk', { generation, id, data: [], reset: true });
      let finished = false;
      return {
        update: bytes => invoke('hash_attachment_chunk', { generation, id, data: Array.from(bytes), reset: false }),
        finish: async () => { finished = true; return invoke<string>('finish_attachment_hash', { generation, id }); },
        abort: () => { if (!finished) { finished = true; void invoke('abort_attachment_hash', { generation, id }).catch(() => undefined); } },
      } satisfies HashSink;
    },
  }), [generation, current]);
  const state = useSyncExternalStore(model.subscribe, model.snapshot, model.snapshot);
  const presence = useRef({ model, active: true });
  useLayoutEffect(() => { if (session.phase === 'closed') model.close(closedNotice(session)); }, [model, session]);
  useEffect(() => {
    presence.current = { model, active: true };
    let disposed = false, draining = false;
    const drain = async () => {
      if (disposed || draining || !generation || current.current.generation !== generation) return;
      draining = true;
      try {
        const result = await invoke<{ generation: number; events: ChatEvent[] }>('drain_chat_events', { generation });
        if (!disposed && result.generation === generation && current.current.generation === generation) for (const event of result.events) await model.receive(event);
      } catch { /* Session lifecycle publishes safe transport errors. */ }
      finally { draining = false; }
    };
    const stop = nativeListener('ssc-chat-event', () => { void drain(); }, undefined, () => { void drain(); });
    const stopSession = nativeListener<SessionState>('ssc-session', event => {
      if (event.payload.generation >= generation && (event.payload.phase === 'closed' || event.payload.generation !== generation)) model.close(closedNotice(event.payload));
    });
    const timer = setInterval(() => { void drain(); }, 100);
    return () => {
      disposed = true; stop(); stopSession(); clearInterval(timer);
      if (presence.current.model === model) presence.current.active = false;
      queueMicrotask(() => { if (presence.current.model !== model || !presence.current.active) model.close(); });
    };
  }, [model, generation, current]);
  useEffect(() => {
    if (session.phase === 'connected') void model.connected(playableVoiceFormats());
  }, [model, session.phase]);
  useEffect(() => { let active = true; void invoke<CaptureStatus>('get_capture_protection').then(value => { if (active) setCapture(value); }).catch(() => { if (active) setCapture({ platform: 'desktop', enabled: false, limitation: 'Ограничение захвата недоступно' }); }); return () => { active = false; }; }, []);
  useEffect(() => {
    let disposed = false, pending = false;
    setRemainingSeconds(null);
    const update = async () => {
      if (pending || !generation || session.phase === 'closed') return;
      pending = true;
      try {
        const room = await invoke<{ generation: number; remaining_seconds: number | null } | null>('get_temporary_room_state');
        if (!disposed && current.current.generation === generation) setRemainingSeconds(room?.generation === generation && typeof room.remaining_seconds === 'number' && Number.isFinite(room.remaining_seconds) ? Math.max(0, Math.min(3600, room.remaining_seconds)) : null);
      } catch { /* Native monotonic enforcement remains authoritative. */ }
      finally { pending = false; }
    };
    void update(); const timer = setInterval(() => { void update(); }, 1000);
    return () => { disposed = true; clearInterval(timer); };
  }, [generation, session.phase, current]);
  const exit = async () => { model.close('Вы вышли. История чата на этом устройстве очищена.'); if (current.current.generation === generation) await invoke('disconnect', { reason: 'exit', generation }); };
  const authorizeVoice = async () => {
    if (current.current.generation !== generation || current.current.phase !== 'connected' || model.snapshot().closed) throw new Error('Chat unavailable');
    await invoke('request_voice_capture', { generation });
  };
  return { model, state, capture, session, error, exit, authorizeVoice, remainingSeconds };
}
