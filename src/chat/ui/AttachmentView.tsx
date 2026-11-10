import { useLayoutEffect, useRef } from 'react';
import { MediaPlayer } from './MediaPlayer';
import type { Attachment, ChatItem } from '../types';
import { formatBytes, previewKind, safeFileName } from './media';
const status: Record<Attachment['status'], string> = { offered: 'Ожидает согласия', waiting: 'Ожидаем собеседника', receiving: 'Получаем', sending: 'Передаём', complete: 'Передача завершена', rejected: 'Отклонено', cancelled: 'Отменено', failed: 'Ошибка передачи', forgotten: 'Удалено из памяти' };
export function AttachmentView({ item, ready, accept, reject, cancel, fail }: { item: ChatItem; ready: boolean; accept(): void; reject(): void; cancel(): void; fail(message: string): void }) {
  const attachment = item.attachment!;
  const downloads = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  useLayoutEffect(() => {
    const owned = downloads.current;
    const clear = () => { for (const [value, timer] of owned) { clearTimeout(timer); URL.revokeObjectURL(value); } owned.clear(); };
    window.addEventListener('pagehide', clear);
    return () => { window.removeEventListener('pagehide', clear); clear(); };
  }, []);
  const kind = previewKind(attachment.mime, attachment.kind), url = attachment.url?.startsWith('blob:') ? attachment.url : null;
  const complete = attachment.status === 'complete', pending = ['offered', 'waiting', 'receiving', 'sending'].includes(attachment.status);
  const progress = attachment.size > 0 ? Math.min(100, Math.max(0, Math.round(attachment.progress * 100 / attachment.size))) : complete ? 100 : 0;
  const save = () => {
    if (!attachment.file) { fail('Файл недоступен. Возможно, он уже удалён из памяти.'); return; }
    // Saving originals is separate from preview eligibility: binary/HEIC/documents get no embedded URL.
    const value = URL.createObjectURL(attachment.file), anchor = document.createElement('a');
    anchor.href = value; anchor.download = safeFileName(attachment.name); anchor.rel = 'noopener';
    try { anchor.click(); }
    catch { URL.revokeObjectURL(value); fail('Не удалось сохранить файл. Проверьте разрешения приложения.'); return; }
    const timer = setTimeout(() => { URL.revokeObjectURL(value); downloads.current.delete(value); }, 1000);
    downloads.current.set(value, timer);
  };
  return <div className="ssc-attachment">
    <div className="ssc-file-heading"><strong>{attachment.name}</strong><span>{formatBytes(attachment.size)}</span></div>
    <p className="ssc-meta">{attachment.status === 'waiting' && item.status === 'pending' ? 'Подготавливаем файл' : status[attachment.status]}{['receiving', 'sending'].includes(attachment.status) ? ` · ${progress}%` : ''}</p>
    {pending && <progress aria-label={`Передача ${attachment.name}`} value={progress} max={100} />}
    {!item.own && attachment.status === 'offered' && <div className="ssc-actions"><button disabled={!ready} onClick={accept}>Принять файл</button><button className="ssc-secondary" disabled={!ready} onClick={reject}>Отклонить</button></div>}
    {pending && attachment.status !== 'offered' && <button className="ssc-secondary" disabled={!ready} onClick={cancel}>Отменить передачу</button>}
    {complete && url && kind === 'photo' && <img className="ssc-photo" src={url} alt={attachment.name} loading="lazy" />}
    {complete && url && kind === 'audio' && <MediaPlayer kind="audio" url={url} label={`Аудио ${attachment.name}`} />}
    {complete && url && kind === 'video' && <MediaPlayer kind="video" url={url} label={`Видео ${attachment.name}`} />}
    {complete && <div className="ssc-actions"><button className="ssc-secondary" onClick={save} disabled={!attachment.file}>Сохранить файл</button>{!kind && <span className="ssc-meta">Предпросмотр не поддерживается. Сохраните и откройте файл вручную.</span>}</div>}
  </div>;
}
