import { invoke } from '@tauri-apps/api/core';
import { listen, type EventCallback, type UnlistenFn } from '@tauri-apps/api/event';
import { useEffect, useRef, useState } from 'react';

export interface AppError {
  code: string;
  stage: string;
  message: string;
  retryable: boolean;
}
export interface SessionState {
  generation: number;
  phase: 'idle' | 'invitation' | 'ice' | 'key_exchange' | 'verification' | 'waiting_peer' | 'connected' | 'recovering' | 'closed';
  route: 'direct' | 'relay' | null;
  close_reason?: 'local_left' | 'peer_left' | 'expired' | 'connection_lost' | null;
}
export interface VerificationCode { generation: number; sas: string }
export const initialSession: SessionState = { generation: 0, phase: 'idle', route: null };
export const phaseLabels: Record<SessionState['phase'], string> = {
  idle: 'Нет подключения', invitation: 'Обмен приглашениями', ice: 'Поиск сетевого маршрута',
  key_exchange: 'Обмен ключами', verification: 'Сверка кода', waiting_peer: 'Ожидаем подтверждения собеседника',
  connected: 'Подключено', recovering: 'Восстановление соединения', closed: 'Соединение закрыто',
};

export function errorMessage(error: unknown, fallback = 'Не удалось выполнить действие'): string {
  if (typeof error === 'object' && error !== null && 'message' in error && typeof error.message === 'string') {
    return error.message;
  }
  return fallback;
}

// Registration is asynchronous: disposal must also remove listeners which arrive later.
export function nativeListener<T>(name: string, callback: EventCallback<T>, onError?: () => void, onReady?: () => void): UnlistenFn {
  let disposed = false;
  let unlisten: UnlistenFn | undefined;
  void listen<T>(name, event => { if (!disposed) callback(event); }).then(fn => {
    if (disposed) fn(); else { unlisten = fn; onReady?.(); }
  }).catch(() => { if (!disposed) onError?.(); });
  return () => { disposed = true; unlisten?.(); };
}

export function useSession() {
  const [session, setSession] = useState(initialSession);
  const [error, setError] = useState<string | null>(null);
  const current = useRef(initialSession);
  useEffect(() => {
    let disposed = false;
    let eventSeen = false;
    const apply = (next: SessionState) => {
      if (next.generation < current.current.generation) return;
      if (next.generation > current.current.generation) setError(null);
      current.current = next;
      setSession(next);
    };
    const stop = nativeListener<SessionState>('ssc-session', event => {
      eventSeen = true;
      apply(event.payload);
    }, () => setError('Не удалось подписаться на состояние соединения'), () => {
      void invoke<SessionState>('get_session_state').then(snapshot => {
        // An event can belong to the session being replaced. A newer snapshot
        // remains authoritative; an equal-generation snapshot must not rewind it.
        if (!disposed && (!eventSeen || snapshot.generation > current.current.generation)) apply(snapshot);
      }).catch(() => { if (!disposed) setError('Не удалось получить состояние соединения'); });
    });
    const stopError = nativeListener<{ generation: number; error: AppError }>('ssc-session-error', event => {
      if (current.current.generation === event.payload.generation) setError(event.payload.error.message);
    });
    return () => { disposed = true; stop(); stopError(); };
  }, []);
  return { session, current, error };
}
