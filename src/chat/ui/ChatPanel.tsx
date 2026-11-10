import { useEffect, useLayoutEffect, useRef, useState, useCallback, useId, type FormEvent, type ReactNode } from 'react';
import { ArrowLeft, Paperclip, Mic, Square, Send, EyeOff, ShieldCheck, Info, Trash2 } from 'lucide-react';
import type { ChatModel } from '../model';
import { MAX_CACHE_BYTES, MAX_FILE_BYTES, REACTIONS, type CaptureStatus, type ChatItem, type ConversationSnapshot } from '../types';
import { ThemeControl } from '../theme';
import { useVoice } from '../voice/useVoice';
import { AttachmentView } from './AttachmentView';
import { formatBytes, MEDIA_WARNING } from './media';
import { HelpDialog, type HelpSource } from './HelpDialog';
import { Modal } from './Modal';
import { MediaPlayer } from './MediaPlayer';
function Dust() { return <div className="ssc-dust" aria-hidden="true">{Array.from({ length: 20 }, (_, index) => <i key={index} style={{ '--particle': index } as React.CSSProperties} />)}</div>; }
function RemainingTime({ seconds }: { seconds: number }) {
  const value = Math.max(0, Math.ceil(seconds)), level = value <= 60 ? 'danger' : value <= 300 ? 'warning' : 'normal';
  const progress = Math.max(0, Math.min(100, value / 3600 * 100));
  return <div className={`ssc-time-pill ssc-time-${level}`} title="Оставшееся время временного чата">
    <div role="timer" aria-live="off" className="ssc-time-display"><svg className="ssc-time-ring" viewBox="0 0 24 24" aria-hidden="true"><circle className="ssc-time-track" cx="12" cy="12" r="9" /><circle className="ssc-time-arc" cx="12" cy="12" r="9" pathLength="100" strokeDasharray="100" strokeDashoffset={100 - progress} /></svg><span className="ssc-time-caption">Осталось</span><time aria-label="Оставшееся время">{String(Math.floor(value / 60)).padStart(2, '0')}:{String(value % 60).padStart(2, '0')}</time></div>
    <span className="ssc-sr-only" role={level === 'danger' ? 'alert' : 'status'}>{level === 'danger' ? 'До завершения чата меньше минуты.' : level === 'warning' ? 'До завершения чата меньше пяти минут.' : ''}</span>
  </div>;
}
const states = { pending: 'Отправляется', sent: 'Передано каналу', delivered: 'Получено собеседником', failed: 'Передача не завершена' };
interface Props { model: ChatModel; state: ConversationSnapshot; onExit(): void; remainingSeconds: number | null; capture?: CaptureStatus; ready?: boolean; onOpenHelpSource?(source: HelpSource): Promise<void>; onBeforeVoiceCapture?(): Promise<void>; children?: ReactNode }
export function ChatPanel({ model, state, onExit, remainingSeconds, capture, ready = false, onOpenHelpSource, onBeforeVoiceCapture, children }: Props) {
  const [text, setText] = useState(''), [sending, setSending] = useState(false), [error, setError] = useState<string | null>(null);
  const [help, setHelp] = useState(false), [masked, setMasked] = useState(() => document.visibilityState === 'hidden'), [retired, setRetired] = useState(false), [dust, setDust] = useState(0);
  const [exitConfirm, setExitConfirm] = useState(false);
  const stay = useRef<HTMLButtonElement>(null), exitDescription = useId();
  const [files, setFiles] = useState<File[]>([]), [offering, setOffering] = useState(false);
  const list = useRef<HTMLOListElement>(null), following = useRef(true), previousCount = useRef(state.messages.length);
  const [newMessages, setNewMessages] = useState(false), [authorizingVoice, setAuthorizingVoice] = useState(false);
  const privacyEpoch = useRef(0), availableNow = useRef(false);
  const picker = useRef<HTMLInputElement>(null), bottom = useRef<HTMLLIElement>(null), sendingNow = useRef(false), mounted = useRef(true);
  const voice = useVoice();
  const discardVoice = voice.discard;
  const closed = state.closed || retired, available = ready && !closed && !masked;
  availableNow.current = available;
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useLayoutEffect(() => { if (closed) { privacyEpoch.current++; setAuthorizingVoice(false); discardVoice(); setText(''); setFiles([]); setHelp(false); setExitConfirm(false); setError(null); } }, [closed, discardVoice]);
  useEffect(() => {
    const conceal = () => { privacyEpoch.current++; setAuthorizingVoice(false); discardVoice(); setFiles([]); setMasked(true); };
    const hidden = () => { if (document.visibilityState === 'hidden') conceal(); };
    document.addEventListener('visibilitychange', hidden); window.addEventListener('pagehide', conceal);
    return () => { document.removeEventListener('visibilitychange', hidden); window.removeEventListener('pagehide', conceal); };
  }, [discardVoice]);
  useEffect(() => { if (exitConfirm) stay.current?.focus(); }, [exitConfirm]);
  const scrollToEnd = useCallback(() => {
    following.current = true; setNewMessages(false);
    const reduced = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
    bottom.current?.scrollIntoView?.({ behavior: reduced ? 'auto' : 'smooth', block: 'end' });
  }, []);
  useEffect(() => {
    const increased = state.messages.length > previousCount.current; previousCount.current = state.messages.length;
    if (masked || closed) return;
    if (following.current) scrollToEnd(); else if (increased) setNewMessages(true);
  }, [state.messages.length, masked, closed, scrollToEnd]);
  const fileCapacity = () => {
    const outgoing = model.snapshot().messages.filter(message => message.own && message.attachment && ['offered', 'waiting', 'sending'].includes(message.attachment.status));
    const pending = outgoing.filter(message => message.attachment?.status !== 'sending').length;
    return Math.max(0, Math.min(4 - pending, 5 - outgoing.length));
  };
  const perform = async (operation: () => void | Promise<void>) => { try { await operation(); } catch { if (mounted.current && !closed) setError('Не удалось выполнить действие. Проверьте соединение.'); } };
  const send = async (event: FormEvent) => {
    event.preventDefault(); if (!available || sendingNow.current || !text.trim()) return;
    if (new TextEncoder().encode(text).length > 256 * 1024) { setError('Сообщение превышает 256 КиБ'); return; }
    sendingNow.current = true; setSending(true); setError(null); const submitted = text;
    try {
      await model.sendText(submitted);
      const item = model.snapshot().messages.filter(message => message.own && message.text === submitted).at(-1);
      if (mounted.current && item?.status !== 'failed') setText(current => current === submitted ? '' : current);
    } catch { if (mounted.current) setError('Не удалось передать сообщение. Текст сохранён.'); }
    finally { sendingNow.current = false; if (mounted.current) setSending(false); }
  };
  const selectFiles = (selected: FileList | null) => {
    const values = Array.from(selected || []); if (picker.current) picker.current.value = '';
    if (!available || !values.length) return;
    if (values.length > 4 || values.some(file => file.size > MAX_FILE_BYTES)) { setError('Выберите не больше четырёх файлов, каждый до 100 МиБ.'); return; }
    if (values.length > fileCapacity()) { setError('Недостаточно свободных мест в очереди. Завершите или отмените ожидающие передачи.'); return; }
    if (values.reduce((size, file) => size + file.size, 0) + state.cacheBytes > MAX_CACHE_BYTES) { setError('Недостаточно свободного места в буфере. Удалите вложения из памяти.'); return; }
    setFiles(values); setError(null);
  };
  const offer = async () => {
    if (!available || offering) return;
    if (files.length > fileCapacity()) { setError('Очередь заполнена. Выбранные файлы остаются для повторной отправки.'); return; }
    setOffering(true);
    try { await model.offerFiles(files); if (mounted.current) setFiles([]); }
    catch { if (mounted.current) setError('Не удалось предложить файлы. Проверьте соединение.'); }
    finally { if (mounted.current) setOffering(false); }
  };
  const sendVoice = async () => {
    if (!available || !voice.draft || offering) return;
    if (!fileCapacity() || model.snapshot().cacheBytes + voice.draft.blob.size > MAX_CACHE_BYTES) { setError('Буфер или очередь заполнены. Запись остаётся для повторной отправки.'); return; }
    setOffering(true); const previous = new Set(model.snapshot().messages.map(message => message.id));
    try {
      const file = new File([voice.draft.blob], `Голосовое сообщение.${voice.draft.extension}`, { type: voice.draft.mime });
      await model.offerFiles([file], 'audio');
      const offered = model.snapshot().messages.find(message => message.own && !previous.has(message.id) && message.attachment);
      if (offered && offered.attachment?.status !== 'failed') voice.discard();
      else if (mounted.current && !model.snapshot().closed) setError('Не удалось предложить запись. Она остаётся для повторной отправки.');
    } catch { if (mounted.current) setError('Не удалось предложить голосовое сообщение. Запись остаётся в предпросмотре.'); }
    finally { if (mounted.current) setOffering(false); }
  };
  const startVoice = async () => {
    if (!availableNow.current || authorizingVoice || voice.phase !== 'idle') return;
    const epoch = ++privacyEpoch.current; setAuthorizingVoice(true); setError(null);
    try {
      await onBeforeVoiceCapture?.();
      if (mounted.current && epoch === privacyEpoch.current && availableNow.current && !model.snapshot().closed && document.visibilityState !== 'hidden') await voice.start(state.peerAudioFormats);
    } catch { if (mounted.current && epoch === privacyEpoch.current) setError('Не удалось разрешить запись. Проверьте доступ к микрофону.'); }
    finally { if (mounted.current && epoch === privacyEpoch.current) setAuthorizingVoice(false); }
  };
  const requestExit = () => { if (closed) { onExit(); return; } setExitConfirm(true); };
  const leave = () => { setExitConfirm(false); privacyEpoch.current++; setAuthorizingVoice(false); voice.discard(); setText(''); setFiles([]); setRetired(true); onExit(); };
  const conceal = () => { privacyEpoch.current++; setAuthorizingVoice(false); voice.discard(); setFiles([]); setMasked(true); };
  const forget = (id: string) => { model.forget(id); setDust(value => value + 1); };
  const message = (item: ChatItem) => <li key={item.id} className={item.own ? 'ssc-message ssc-own' : 'ssc-message'}>
    <div className="ssc-message-heading"><span title={item.own ? undefined : 'Случайное имя для этого чата'} aria-description={item.own ? undefined : 'Случайное имя для этого чата'}>{item.own ? 'Вы' : model.peerName}</span><button className="ssc-icon-button" disabled={!!item.attachment && ['offered', 'sending', 'receiving', 'waiting'].includes(item.attachment.status)} aria-label="Удалить сообщение из памяти" title="Удалить только на этом устройстве; активную передачу сначала отмените" onClick={() => forget(item.id)}><Trash2 size={14} /></button></div>
    {item.text !== undefined && <p className="ssc-message-text">{item.text}</p>}
    {item.attachment && <AttachmentView item={item} ready={available} accept={() => { void perform(() => model.accept(item.attachment!.id)); }} reject={() => { void perform(() => model.reject(item.attachment!.id)); }} cancel={() => { void perform(() => model.cancel(item.attachment!.id)); }} fail={setError} />}
    <div className="ssc-message-footer"><span>{states[item.status]}</span><div className="ssc-reactions" role="group" aria-label="Реакции на сообщение">{REACTIONS.map(emoji => {
      const own = item.reactions.own === emoji, peer = item.reactions.peer === emoji;
      return <button key={emoji} className={own ? 'ssc-reaction selected' : 'ssc-reaction'} aria-label={`Реакция ${emoji}`} aria-pressed={own} disabled={!available} onClick={() => { void perform(() => model.react(item.id, own ? null : emoji)); }}>{emoji}{own || peer ? <small>{Number(own) + Number(peer)}</small> : null}</button>;
    })}</div></div>
  </li>;
  return <section className={capture?.platform === 'browser' ? 'ssc-chat ssc-chat-browser' : 'ssc-chat'} aria-label="Секретный чат">
    <header className="ssc-chat-header">
      <button className="ssc-icon-button ssc-header-back" aria-label="Выйти из чата" onClick={requestExit}><ArrowLeft size={20} /></button>
      <span className="ssc-brand-mark" aria-hidden="true">◈</span>
      <div className="ssc-chat-title"><h1 data-ssc-peer-name={closed ? undefined : true} title={closed ? undefined : 'Случайное имя для этого чата'} aria-description={closed ? undefined : 'Случайное имя для этого чата'}>{closed ? 'Чат завершён' : model.peerName}</h1><span><ShieldCheck size={12} />{closed ? 'История не сохраняется' : ready ? 'Сверка подтверждена' : 'Ожидаем соединения…'}</span></div>
      {!closed && remainingSeconds !== null && <RemainingTime seconds={remainingSeconds} />}
      <div className="ssc-header-actions"><ThemeControl /><button className="ssc-icon-button" aria-label="Как защищён чат" onClick={() => setHelp(true)}><Info size={19} /></button>{!closed && <button className="ssc-icon-button" aria-label="Скрыть чат" onClick={conceal}><EyeOff size={19} /></button>}{capture?.platform === 'browser' && !closed && <button className="ssc-header-exit" aria-label="Выйти и завершить чат" onClick={requestExit}>Выйти</button>}</div>
    </header>
    {!closed && children}
    {closed ? <div className="ssc-closed"><Dust /><span className="ssc-closed-mark" aria-hidden="true">◈</span><h2>Разговор завершён</h2><p>{state.notice || 'История этого разговора больше не доступна в приложении.'}</p><p className="ssc-meta">Сохранённые получателем копии и файлы вне приложения не удаляются.</p></div> : masked ? <div className="ssc-curtain"><EyeOff size={36} /><h2>Чат скрыт</h2><p>Шторка скрывает интерфейс. Соединение остаётся активным.</p><button onClick={() => setMasked(false)}>Показать чат</button></div> : <>
      {(error || state.notice) && <p className="ssc-notice" role="alert">{error || state.notice}</p>}
      <ol ref={list} className="ssc-messages" aria-label="Переписка" aria-live="polite" aria-relevant="additions" onScroll={() => { const element = list.current; if (element) { following.current = element.scrollHeight - element.scrollTop - element.clientHeight < 48; if (following.current) setNewMessages(false); } }}>
        {state.messages.length ? state.messages.map(message) : <li className="ssc-empty"><ShieldCheck size={24} /><strong>Можно начинать разговор</strong><p>Переписка шифруется между устройствами.</p><small>Не забудьте, что собеседник может сохранить копию.</small></li>}
        <li className="ssc-scroll-end" aria-hidden="true" ref={bottom} />
      </ol>
      {newMessages && <button className="ssc-new-messages" onClick={scrollToEnd}>Новые сообщения ↓</button>}
      {dust > 0 && <Dust key={dust} />}
      {authorizingVoice && voice.phase === 'idle' && <p className="ssc-notice" role="status">Подготавливаем доступ к микрофону…</p>}
      {voice.phase !== 'idle' && <div className="ssc-voice">
        {voice.phase === 'permission' && <p role="status">Ожидаем разрешение на микрофон…</p>}
        {voice.phase === 'recording' && <div className="ssc-actions"><span className="ssc-recording-dot" aria-hidden="true" /><strong role="status">Запись {Math.floor(voice.elapsed / 60)}:{String(voice.elapsed % 60).padStart(2, '0')} / 5:00</strong><button onClick={() => { void voice.stop(); }}><Square size={15} />Остановить запись</button></div>}
        {voice.phase === 'stopping' && <p role="status">Готовим предпросмотр…</p>}
        {voice.draft && <><MediaPlayer kind="audio" url={voice.draft.url} label="Предпросмотр голосового сообщения" /><p className="ssc-media-warning">{MEDIA_WARNING}.</p><div className="ssc-actions"><button disabled={!available || offering} onClick={() => { void sendVoice(); }}>Отправить голосовое</button><button className="ssc-secondary" onClick={voice.discard}>Удалить запись</button></div></>}
        {!voice.draft && <button className="ssc-secondary" onClick={voice.discard}>Отменить запись</button>}
      </div>}
      {voice.error && <p className="ssc-notice" role="alert">{voice.error}</p>}
      <form className="ssc-composer" onSubmit={event => { void send(event); }}>
        <input ref={picker} type="file" multiple className="ssc-sr-only" aria-label="Выбрать вложения" tabIndex={-1} onChange={event => selectFiles(event.target.files)} />
        <button type="button" className="ssc-icon-button" aria-label="Прикрепить файл" title="Фото, видео или файл до 100 МиБ" disabled={!available || offering} onClick={() => picker.current?.click()}><Paperclip size={20} /></button>
        <label className="ssc-sr-only" htmlFor="chat-text">Сообщение</label><textarea id="chat-text" aria-label="Сообщение" placeholder="Напишите сообщение…" rows={1} value={text} onChange={event => setText(event.target.value)} disabled={!available || sending} maxLength={256 * 1024} />
        <button type="button" className="ssc-icon-button" aria-label="Записать голосовое" disabled={!available || authorizingVoice || voice.phase !== 'idle'} onClick={() => { void startVoice(); }}><Mic size={20} /></button>
        <button className="ssc-send" aria-label="Отправить" type="submit" disabled={!available || sending || !text.trim()}><Send size={19} /><span>Отправить</span></button>
      </form>
      <div className="ssc-chat-footnote"><p>{MEDIA_WARNING}. Исходные файлы не очищаются.</p><span>Буфер вложений {formatBytes(state.cacheBytes)} / 200 МиБ · запись до 5 минут, скрытие окна её прервёт</span></div>
    </>}
    {exitConfirm && !closed && <Modal title="Завершить чат?" onClose={() => setExitConfirm(false)}><p id={exitDescription} className="ssc-exit-warning">После выхода история и несохранённые файлы на этом устройстве будут очищены. Восстановить разговор нельзя. Активные передачи и запись будут остановлены.</p><p className="ssc-meta">Сохранённые собеседником копии останутся у него.</p><div className="ssc-exit-actions"><button ref={stay} className="ssc-secondary" aria-describedby={exitDescription} onClick={() => setExitConfirm(false)}>Остаться</button><button className="ssc-danger-button" onClick={leave}>Выйти и очистить</button></div></Modal>}
    {help && <HelpDialog onClose={() => setHelp(false)} capture={capture} openSource={onOpenHelpSource} />}
    {!closed && files.length > 0 && <Modal title="Отправить вложения?" onClose={() => { if (!offering) setFiles([]); }}><p className="ssc-media-warning">{MEDIA_WARNING}.</p><p className="ssc-meta">Мы передаём оригиналы без удаления EXIF и других метаданных. При необходимости подготовьте отдельную копию.</p><ul className="ssc-selected-files">{files.map((file, index) => <li key={index}><span>{file.name}</span><small>{formatBytes(file.size)}</small></li>)}</ul><div className="ssc-actions"><button disabled={!available || offering} onClick={() => { void offer(); }}>{offering ? 'Подготавливаем…' : 'Отправить файлы'}</button><button className="ssc-secondary" disabled={offering} onClick={() => setFiles([])}>Отмена</button></div></Modal>}
  </section>;
}
