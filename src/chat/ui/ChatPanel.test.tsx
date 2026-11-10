import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ChatPanel } from './ChatPanel';
import { AttachmentView } from './AttachmentView';
import { MEDIA_WARNING, previewKind } from './media';
import { ThemeProvider } from '../theme';
import type { ChatModel } from '../model';
import type { ChatItem, ConversationSnapshot } from '../types';
const initial = (): ConversationSnapshot => ({ messages: [], closed: false, notice: null, cacheBytes: 0, peerAudioFormats: ['audio/wav'] });
const item = (text = 'private fixture phrase'): ChatItem => ({ id: 'a'.repeat(32), text, own: true, status: 'sent', timestamp: 0, reactions: { own: null, peer: null } });
function fake(state = initial()) {
  return { peerName: 'Тихий Енот', snapshot: () => state, sendText: vi.fn().mockResolvedValue(undefined), offerFiles: vi.fn().mockResolvedValue(undefined), accept: vi.fn(), reject: vi.fn(), cancel: vi.fn(), react: vi.fn(), forget: vi.fn() } as unknown as ChatModel;
}
beforeEach(() => {
  localStorage.clear();
  vi.stubGlobal('matchMedia', vi.fn().mockReturnValue({ matches: true, addEventListener: vi.fn(), removeEventListener: vi.fn() }));
  vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockReturnValue('maybe');
  vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => undefined);
  vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => undefined);
});
it('keeps the local peer alias through theme and message renders without presenting it as verification', () => {
  const incoming = { ...item('incoming fixture'), id: 'c'.repeat(32), own: false }, state = { ...initial(), messages: [item(), incoming] }, model = fake(state);
  const rendered = render(<ThemeProvider><ChatPanel model={model} state={state} ready remainingSeconds={3599} onExit={vi.fn()} /></ThemeProvider>);
  const heading = screen.getByRole('heading', { name: model.peerName });
  expect(heading).toHaveAttribute('data-ssc-peer-name'); expect(heading).toHaveAccessibleDescription('Случайное имя для этого чата');
  expect(screen.getAllByText(model.peerName)).toHaveLength(2); expect(screen.getByText('Вы')).toBeInTheDocument();
  expect(screen.getByText('Сверка подтверждена')).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText('Тема'), { target: { value: 'light' } });
  rendered.rerender(<ThemeProvider><ChatPanel model={model} state={{ ...state, messages: [...state.messages, { ...incoming, id: 'b'.repeat(32) }] }} ready remainingSeconds={3598} onExit={vi.fn()} /></ThemeProvider>);
  expect(screen.getByRole('heading', { name: model.peerName })).toBe(heading); expect(screen.getAllByText(model.peerName)).toHaveLength(3);
  expect(localStorage.length).toBe(1); expect(localStorage.getItem('ssc-theme')).toBe('light');
  rendered.rerender(<ThemeProvider><ChatPanel model={model} state={{ ...state, messages: [], closed: true }} remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  expect(screen.getByRole('heading', { name: 'Чат завершён' })).not.toHaveAttribute('data-ssc-peer-name');
  expect(screen.queryByText(model.peerName)).not.toBeInTheDocument();
});
it('clears plaintext immediately on close and dust contains no message data', () => {
  const state = { ...initial(), messages: [item()] }, model = fake(state);
  const rendered = render(<ThemeProvider><ChatPanel model={model} state={state} ready remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  expect(screen.getByText('private fixture phrase')).toBeInTheDocument();
  rendered.rerender(<ThemeProvider><ChatPanel model={model} state={{ ...state, messages: [], closed: true }} ready={false} remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  expect(screen.queryByText('private fixture phrase')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('Сообщение')).not.toBeInTheDocument();
  expect(document.querySelector('.ssc-dust')).toHaveTextContent('');
  expect(document.querySelector('canvas')).toBeNull();
});
it('manual curtain removes content and drafts from the DOM until explicit reveal', () => {
  const state = { ...initial(), messages: [item()] };
  render(<ThemeProvider><ChatPanel model={fake(state)} state={state} ready remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  fireEvent.change(screen.getByLabelText('Сообщение'), { target: { value: 'unsent private draft' } });
  fireEvent.click(screen.getByRole('button', { name: 'Скрыть чат' }));
  expect(screen.queryByText('private fixture phrase')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('Сообщение')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Показать чат' }));
  expect(screen.getByLabelText('Сообщение')).toHaveValue('unsent private draft');
});
it('theme defaults to auto without storage and only explicit theme preferences persist', () => {
  render(<ThemeProvider><ChatPanel model={fake()} state={initial()} ready remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  expect(screen.getByLabelText('Тема')).toHaveValue('auto'); expect(localStorage.length).toBe(0);
  fireEvent.change(screen.getByLabelText('Тема'), { target: { value: 'light' } });
  expect(document.querySelector('[data-ssc-theme]')).toHaveAttribute('data-ssc-theme', 'light');
  expect(localStorage.getItem('ssc-theme')).toBe('light'); expect(localStorage.length).toBe(1);
  fireEvent.change(screen.getByLabelText('Тема'), { target: { value: 'auto' } });
  expect(localStorage.length).toBe(0);
});
it('asks for media confirmation with metadata warning before offering originals', async () => {
  const model = fake(), file = new File(['original fixture bytes'], 'photo.jpg', { type: 'image/jpeg' });
  render(<ThemeProvider><ChatPanel model={model} state={initial()} ready remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  fireEvent.change(screen.getByLabelText('Выбрать вложения'), { target: { files: [file] } });
  expect(model.offerFiles).not.toHaveBeenCalled();
  expect(screen.getByRole('dialog')).toHaveTextContent(MEDIA_WARNING);
  fireEvent.click(screen.getByRole('button', { name: 'Отправить файлы' }));
  await waitFor(() => expect(model.offerFiles).toHaveBeenCalledWith([file]));
  expect(file.name).toBe('photo.jpg');
});
it('forwards one of six reactions using common message IDs', () => {
  const message = item(), state = { ...initial(), messages: [message] }, model = fake(state);
  render(<ThemeProvider><ChatPanel model={model} state={state} ready remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  const group = screen.getByRole('group', { name: 'Реакции на сообщение' });
  expect(group.querySelectorAll('button')).toHaveLength(6);
  fireEvent.click(screen.getByRole('button', { name: 'Реакция 👍' }));
  expect(model.react).toHaveBeenCalledWith(message.id, '👍');
});
it('does not embed HTML or SVG and never auto-plays media', () => {
  expect(previewKind('text/html', 'file')).toBeNull(); expect(previewKind('image/svg+xml', 'photo')).toBeNull();
  const base = item(); const attachment = { id: base.id, name: 'recording.wav', mime: 'audio/wav', size: 44, sha256: 'a'.repeat(64), kind: 'audio' as const, status: 'complete' as const, progress: 44, url: 'blob:owned-fixture' };
  const rendered = render(<ThemeProvider><AttachmentView item={{ ...base, attachment }} ready accept={vi.fn()} reject={vi.fn()} cancel={vi.fn()} fail={vi.fn()} /></ThemeProvider>);
  const audio = screen.getByLabelText('Аудио recording.wav');
  expect(audio).toHaveAttribute('preload', 'none'); expect(audio).not.toHaveAttribute('autoplay');
  rendered.rerender(<ThemeProvider><AttachmentView item={{ ...base, attachment: { ...attachment, name: 'unknown.svg', mime: 'image/svg+xml', kind: 'photo' } }} ready accept={vi.fn()} reject={vi.fn()} cancel={vi.fn()} fail={vi.fn()} /></ThemeProvider>);
  expect(document.querySelector('img, iframe, object, embed')).toBeNull();
  expect(screen.getByRole('button', { name: 'Сохранить файл' })).toBeInTheDocument();
});
it('shows native capture limits and technical sources without universal screenshot claims', () => {
  render(<ThemeProvider><ChatPanel model={fake()} state={initial()} ready remainingSeconds={null} capture={{ platform: 'linux', enabled: false, limitation: 'Защита окна этой ОС недоступна' }} onExit={vi.fn()} /></ThemeProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Как защищён чат' }));
  const dialog = screen.getByRole('dialog'); expect(dialog).toHaveTextContent('Защита окна этой ОС недоступна');
  expect(screen.getByRole('link', { name: /RFC 7748/ })).toHaveAttribute('href', 'https://www.rfc-editor.org/rfc/rfc7748');
  expect(dialog).toHaveTextContent('TURN'); expect(dialog).toHaveTextContent('не реализует ZRTP');
});
it('honours reduced motion for scrolling and disables sends until ready', () => {
  const scroll = vi.fn(); Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, value: scroll });
  render(<ThemeProvider><ChatPanel model={fake()} state={initial()} remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  expect(scroll).toHaveBeenCalledWith({ behavior: 'auto', block: 'end' });
  expect(screen.getByRole('button', { name: 'Отправить' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Записать голосовое' })).toBeDisabled();
});
it('does not jump away from older messages and offers an explicit new-message button', () => {
  const scroll = vi.fn(); Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, value: scroll });
  const first = item(), state = { ...initial(), messages: [first] }, model = fake(state);
  const rendered = render(<ThemeProvider><ChatPanel model={model} state={state} ready remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  const list = screen.getByLabelText('Переписка');
  Object.defineProperties(list, { scrollHeight: { value: 1000, configurable: true }, clientHeight: { value: 200, configurable: true }, scrollTop: { value: 100, writable: true, configurable: true } });
  fireEvent.scroll(list); scroll.mockClear();
  rendered.rerender(<ThemeProvider><ChatPanel model={model} state={{ ...state, messages: [first, { ...item('another fixture message'), id: 'b'.repeat(32) }] }} ready remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  expect(scroll).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Новые сообщения ↓' }));
  expect(scroll).toHaveBeenCalled(); expect(screen.queryByRole('button', { name: 'Новые сообщения ↓' })).not.toBeInTheDocument();
});
it('pauses and detaches buffered playback on curtain and close', () => {
  const message = { ...item(), text: undefined, attachment: { id: 'b'.repeat(32), name: 'audio.wav', mime: 'audio/wav', kind: 'audio' as const, size: 44, sha256: 'a'.repeat(64), status: 'complete' as const, progress: 44, url: 'blob:owned-fixture' } };
  const state = { ...initial(), messages: [message] }, model = fake(state);
  const rendered = render(<ThemeProvider><ChatPanel model={model} state={state} ready remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  const audio = screen.getByLabelText('Аудио audio.wav');
  fireEvent.click(screen.getByRole('button', { name: 'Скрыть чат' }));
  expect(HTMLMediaElement.prototype.pause).toHaveBeenCalled(); expect(HTMLMediaElement.prototype.load).toHaveBeenCalled(); expect(audio).not.toHaveAttribute('src');
  fireEvent.click(screen.getByRole('button', { name: 'Показать чат' }));
  const restored = screen.getByLabelText('Аудио audio.wav');
  rendered.rerender(<ThemeProvider><ChatPanel model={model} state={{ ...state, messages: [], closed: true }} ready={false} remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  expect(restored).not.toHaveAttribute('src');
});
it('refuses a five-file pick instead of silently dropping an unoffered file', () => {
  const model = fake(); render(<ThemeProvider><ChatPanel model={model} state={initial()} ready remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  fireEvent.change(screen.getByLabelText('Выбрать вложения'), { target: { files: Array.from({ length: 5 }, (_, index) => new File(['x'], `${index}.bin`)) } });
  expect(screen.getByRole('alert')).toHaveTextContent('четырёх'); expect(model.offerFiles).not.toHaveBeenCalled(); expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});
it('saves original binary and unsupported HEIC blobs only on explicit user action', () => {
  const create = vi.fn().mockReturnValue('blob:explicit-download'), revoke = vi.fn();
  Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: create }); Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: revoke });
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => undefined);
  for (const [mime, name, kind] of [['application/octet-stream', 'original.bin', 'file'], ['image/heic', 'original.heic', 'photo'], ['audio/unknown', 'original.audio', 'audio']] as const) {
    const file = new Blob([new Uint8Array([1, 2, 3, 4])], { type: mime });
    const base = item(), attachment = { id: base.id, name, mime, size: file.size, sha256: 'a'.repeat(64), kind, status: 'complete' as const, progress: file.size, file };
    const rendered = render(<ThemeProvider><AttachmentView item={{ ...base, attachment }} ready accept={vi.fn()} reject={vi.fn()} cancel={vi.fn()} fail={vi.fn()} /></ThemeProvider>);
    expect(document.querySelector('img, video, audio, iframe, object, embed')).toBeNull();
    const before = create.mock.calls.length;
    const save = screen.getByRole('button', { name: 'Сохранить файл' }); expect(save).toBeEnabled();
    expect(create).toHaveBeenCalledTimes(before);
    fireEvent.click(save); expect(create).toHaveBeenLastCalledWith(file);
    expect(click.mock.instances.at(-1)).toHaveAttribute('download', name);
    rendered.unmount(); expect(revoke).toHaveBeenCalledWith('blob:explicit-download');
  }
});
it('authorizes a native microphone only after explicit recording intent and before getUserMedia', async () => {
  const order: string[] = [], stopped = vi.fn();
  Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: { getUserMedia: vi.fn(async () => { order.push('microphone'); return { getTracks: () => [{ stop: stopped }] }; }) } });
  class Recorder {
    static isTypeSupported() { return true; }
    state = 'inactive'; onstop = null; ondataavailable = null; onerror = null;
    start() { this.state = 'recording'; } stop() { this.state = 'inactive'; }
  }
  vi.stubGlobal('MediaRecorder', Recorder);
  const authorize = vi.fn(async () => { order.push('authorize'); });
  const rendered = render(<ThemeProvider><ChatPanel model={fake()} state={{ ...initial(), peerAudioFormats: ['audio/webm;codecs=opus'] }} ready remainingSeconds={null} onBeforeVoiceCapture={authorize} onExit={vi.fn()} /></ThemeProvider>);
  expect(authorize).not.toHaveBeenCalled(); expect(navigator.mediaDevices.getUserMedia).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Записать голосовое' }));
  await screen.findByRole('button', { name: 'Остановить запись' }); expect(order).toEqual(['authorize', 'microphone']);
  rendered.unmount(); expect(stopped).toHaveBeenCalled(); vi.unstubAllGlobals();
});
it('does not start a late microphone permission flow after the conversation closes', async () => {
  let allow!: () => void;
  const authorize = vi.fn(() => new Promise<void>(resolve => { allow = resolve; }));
  const getUserMedia = vi.fn(); Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: { getUserMedia } });
  const state = initial(), model = fake(state);
  const rendered = render(<ThemeProvider><ChatPanel model={model} state={state} ready remainingSeconds={null} onBeforeVoiceCapture={authorize} onExit={vi.fn()} /></ThemeProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Записать голосовое' })); await waitFor(() => expect(authorize).toHaveBeenCalledOnce());
  state.closed = true;
  rendered.rerender(<ThemeProvider><ChatPanel model={model} state={state} ready={false} remainingSeconds={null} onBeforeVoiceCapture={authorize} onExit={vi.fn()} /></ThemeProvider>);
  allow(); await waitFor(() => expect(screen.getByRole('heading', { name: 'Разговор завершён' })).toBeInTheDocument());
  expect(getUserMedia).not.toHaveBeenCalled();
});
it('starts behind the privacy curtain when mounted in an already hidden document', () => {
  const descriptor = Object.getOwnPropertyDescriptor(document, 'visibilityState');
  Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' });
  try {
    const state = { ...initial(), messages: [item()] };
    render(<ThemeProvider><ChatPanel model={fake(state)} state={state} ready remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
    expect(screen.getByRole('heading', { name: 'Чат скрыт' })).toBeInTheDocument();
    expect(screen.queryByText('private fixture phrase')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Сообщение')).not.toBeInTheDocument();
  } finally { if (descriptor) Object.defineProperty(document, 'visibilityState', descriptor); else Reflect.deleteProperty(document, 'visibilityState'); }
});
it('requires a deliberate in-app confirmation and focuses Stay without clearing drafts', async () => {
  const onExit = vi.fn(), state = { ...initial(), messages: [item()] };
  render(<ThemeProvider><ChatPanel model={fake(state)} state={state} ready remainingSeconds={3558} onExit={onExit} /></ThemeProvider>);
  fireEvent.change(screen.getByLabelText('Сообщение'), { target: { value: 'preserve this draft' } });
  fireEvent.click(screen.getByRole('button', { name: 'Выйти из чата' }));
  const dialog = screen.getByRole('dialog', { name: 'Завершить чат?' });
  expect(dialog).toHaveTextContent('Восстановить разговор нельзя'); expect(dialog).toHaveTextContent('Активные передачи');
  await waitFor(() => expect(screen.getByRole('button', { name: 'Остаться' })).toHaveFocus());
  expect(onExit).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Остаться' }));
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument(); expect(screen.getByLabelText('Сообщение')).toHaveValue('preserve this draft');
  fireEvent.click(screen.getByRole('button', { name: 'Выйти из чата' }));
  fireEvent.click(screen.getByRole('button', { name: 'Выйти и очистить' }));
  expect(onExit).toHaveBeenCalledOnce(); expect(screen.queryByText('private fixture phrase')).not.toBeInTheDocument(); expect(screen.queryByLabelText('Сообщение')).not.toBeInTheDocument();
});
it('Escape and backdrop cancel exit; authenticated peer close retires an open dialog', () => {
  const state = initial(), model = fake(state), onExit = vi.fn();
  const rendered = render(<ThemeProvider><ChatPanel model={model} state={state} ready remainingSeconds={null} onExit={onExit} /></ThemeProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Выйти из чата' })); fireEvent.keyDown(document, { key: 'Escape' });
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument(); expect(onExit).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Выйти из чата' })); fireEvent.mouseDown(document.querySelector('.ssc-modal-backdrop')!);
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument(); expect(onExit).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Выйти из чата' }));
  rendered.rerender(<ThemeProvider><ChatPanel model={model} state={{ ...state, closed: true, messages: [], notice: 'Собеседник вышел.' }} ready={false} remainingSeconds={null} onExit={onExit} /></ThemeProvider>);
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument(); expect(onExit).not.toHaveBeenCalled();
});
it('keeps the only exit controls in the header and renders a source-driven warning timer', () => {
  const model = fake(), state = initial();
  const rendered = render(<ThemeProvider><ChatPanel model={model} state={state} ready remainingSeconds={3558} capture={{ platform: 'browser', enabled: false, limitation: null }} onExit={vi.fn()} /></ThemeProvider>);
  expect(document.querySelector('.ssc-composer')?.querySelector('[aria-label="Выйти и завершить чат"]')).toBeNull();
  expect(document.querySelector('.ssc-chat-footnote')?.querySelector('button')).toBeNull();
  const wideExit = screen.getByRole('button', { name: 'Выйти и завершить чат' }); expect(wideExit.closest('header')).not.toBeNull();
  expect(screen.getByLabelText('Оставшееся время')).toHaveTextContent('59:18'); expect(screen.getByRole('timer')).toHaveAttribute('aria-live', 'off');
  rendered.rerender(<ThemeProvider><ChatPanel model={model} state={state} ready remainingSeconds={299} onExit={vi.fn()} /></ThemeProvider>);
  expect(document.querySelector('.ssc-time-warning')).not.toBeNull(); expect(screen.getByRole('status')).toHaveTextContent('меньше пяти минут');
  rendered.rerender(<ThemeProvider><ChatPanel model={model} state={state} ready remainingSeconds={60} onExit={vi.fn()} /></ThemeProvider>);
  expect(document.querySelector('.ssc-time-danger')).not.toBeNull(); expect(screen.getByRole('alert')).toHaveTextContent('меньше минуты');
  rendered.rerender(<ThemeProvider><ChatPanel model={model} state={state} ready remainingSeconds={null} onExit={vi.fn()} /></ThemeProvider>);
  expect(screen.queryByLabelText('Оставшееся время')).not.toBeInTheDocument();
});
