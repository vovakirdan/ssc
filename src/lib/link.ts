import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { type AppError, errorMessage, nativeListener } from '@/lib/session';

export interface LinkSnapshot {
  generation: number;
  role: 'creator' | 'guest';
  phase: 'creating' | 'waiting_peer' | 'joining' | 'sending_answer' | 'waiting_connection' | 'complete' | 'error' | 'cancelled';
  url: string | null;
  expires_at: string | null;
  error: AppError | null;
}
export const linkPhaseLabels: Record<LinkSnapshot['phase'], string> = {
  creating: 'Создаём приглашение…',
  waiting_peer: 'Ожидаем собеседника',
  joining: 'Получаем приглашение…',
  sending_answer: 'Передаём ответ…',
  waiting_connection: 'Устанавливаем соединение…',
  complete: 'Соединение установлено',
  error: 'Не удалось завершить подключение',
  cancelled: 'Приглашение отменено',
};

export function useLinkState() {
  const [snapshot, setSnapshot] = useState<LinkSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const current = useRef<LinkSnapshot | null>(null);
  const eventVersion = useRef(0);
  const apply = useCallback((next: LinkSnapshot) => {
    if (current.current && next.generation < current.current.generation) return;
    current.current = next;
    setSnapshot(next);
  }, []);
  useEffect(() => {
    let disposed = false;
    let eventSeen = false;
    const stop = nativeListener<LinkSnapshot>('ssc-link', event => {
      if (current.current && event.payload.generation < current.current.generation) return;
      setError(null);
      eventSeen = true;
      eventVersion.current++;
      apply(event.payload);
    }, () => setError('Не удалось подписаться на подключение по ссылке'), () => {
      void invoke<LinkSnapshot | null>('get_link_state').then(next => {
        if (disposed) return;
        const newerGeneration = next && (!current.current || next.generation > current.current.generation);
        if (!eventSeen || newerGeneration) { setError(null); if (next) apply(next); }
      }).catch(failure => { if (!disposed) setError(errorMessage(failure, 'Не удалось получить состояние приглашения')); });
    });
    return () => { disposed = true; stop(); };
  }, [apply]);
  // A command may return an earlier snapshot after an event has already advanced it.
  const applyCommand = (next: LinkSnapshot, version: number) => {
    if (!current.current || next.generation > current.current.generation || version === eventVersion.current) apply(next);
  };
  return { snapshot, current, eventVersion, applyCommand, error };
}
