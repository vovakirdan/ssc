import { useEffect, useState, useSyncExternalStore } from 'react';
import { ThemeProvider, ThemeControl } from '../../src/chat/theme';
import { ChatPanel } from '../../src/chat/ui/ChatPanel';
import { HelpDialog } from '../../src/chat/ui/HelpDialog';
import { BrowserChat } from './chat';

const labels = {
  home: 'Приватный разговор по одной ссылке', preview: 'Вас пригласили в приватный чат', working: 'Готовим защищённое соединение…',
  waiting: 'Приглашение готово', connecting: 'Соединяемся с собеседником…', verification: 'Сверьте код с собеседником',
  waiting_peer: 'Ожидаем подтверждения собеседника…', chat: 'Защищённый чат', recovering: 'Восстанавливаем связь…', closed: 'Чат завершён',
};
function WebContent() {
  const [chat] = useState(() => new BrowserChat());
  const [view, setView] = useState(() => chat.snapshot());
  const state = useSyncExternalStore(chat.conversation.subscribe, chat.conversation.snapshot, chat.conversation.snapshot);
  const [enteredChat, setEnteredChat] = useState(false), [help, setHelp] = useState(false);
  const [compared, setCompared] = useState(false);
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState(false);
  useEffect(() => {
    const unsubscribe = chat.subscribe(next => { setView(next); if (next.phase === 'chat') setEnteredChat(true); });
    const pagehide = () => chat.leave(true);
    window.addEventListener('pagehide', pagehide);
    return () => { unsubscribe(); window.removeEventListener('pagehide', pagehide); chat.leave(true); };
  }, [chat]);
  const remaining = view.remainingSeconds;
  const copy = async () => {
    try { await navigator.clipboard.writeText(view.invite); setCopied(true); setCopyError(false); }
    catch { setCopyError(true); }
  };
  const active = !['home', 'preview', 'closed'].includes(view.phase);
  const conversation = enteredChat && ['chat', 'recovering', 'closed'].includes(view.phase);
  if (conversation) return <main className="ssc-web-conversation"><ChatPanel model={chat.conversation} state={state} ready={view.phase === 'chat'} remainingSeconds={remaining} capture={{ platform: 'browser', enabled: false, limitation: 'Браузер не может надёжно запретить снимок или запись экрана.' }} onExit={() => chat.leave()} />{view.phase === 'closed' && <a className="button primary" href="/">Создать новый чат</a>}</main>;
  return <main className="shell">
    <header className="brand"><span className="brand-icon" aria-hidden="true">◈</span><div><p>SECRET CHAT</p><span>Без регистрации · один собеседник</span></div><div className="web-header-actions"><ThemeControl /><button className="help-button" aria-label="Как защищён чат" onClick={() => setHelp(true)}>?</button></div></header>
    <section className={`card ${conversation ? 'conversation' : ''}`}>
      <div className="card-heading"><span className="badge">{view.phase === 'chat' ? 'Код подтверждён с двух сторон' : 'Временный чат'}</span><h1>{labels[view.phase]}</h1></div>
      {view.error && <p className="notice" role="alert">{view.error}</p>}
      {view.phase === 'home' && <div className="stack">
        <p>Создайте чат и отправьте ссылку одному человеку. Устанавливать приложение не нужно.</p>
        <button className="primary" onClick={() => { void chat.create(); }}>Создать чат</button>
        <p className="muted">После подключения у вас будет один час. История не сохраняется: после выхода, закрытия или обновления страницы разговор восстановить нельзя.</p>
      </div>}
      {view.phase === 'preview' && <div className="stack">
        <p>Вход начнётся только после нажатия кнопки. Открытие этой страницы само по себе не занимает место собеседника.</p>
        <button className="primary" onClick={() => { void chat.join(); }}>Войти</button>
        <p className="muted">Ссылка предназначена одному человеку. После входа сверьте код через доверенный канал связи.</p>
      </div>}
      {view.phase === 'working' && <p className="muted" role="status">Подготавливаем одноразовые параметры. Обычно это занимает несколько секунд.</p>}
      {view.phase === 'waiting' && <div className="stack">
        <p>Отправьте эту ссылку вашему собеседнику. Её можно использовать для одного подключения.</p>
        <label htmlFor="invitation">Ссылка приглашения</label>
        <input id="invitation" aria-label="Ссылка приглашения" value={view.invite} readOnly onFocus={event => event.currentTarget.select()} />
        <button className="primary" onClick={() => { void copy(); }}>{copied ? 'Ссылка скопирована' : 'Скопировать ссылку'}</button>
        {copyError && <p role="status" className="muted">Выделите ссылку выше и скопируйте её вручную.</p>}
        <p className="muted" role="status">Ожидаем входа собеседника…</p>
      </div>}
      {view.phase === 'connecting' && <p role="status" className="muted">Ищем маршрут и обмениваемся одноразовыми ключами…</p>}
      {['verification', 'waiting_peer'].includes(view.phase) && <div className="stack">
        <p>Позвоните собеседнику или используйте другой доверенный канал. Сравните все символы: на обеих сторонах должен быть одинаковый код.</p>
        <output className="sas" aria-label="Код сверки">{view.sas}</output>
        {view.phase === 'verification' ? <>
          <label className="comparison"><input type="checkbox" checked={compared} onChange={event => setCompared(event.target.checked)} />Я сверил(а) весь код с собеседником</label>
          <button className="primary" disabled={!compared} onClick={() => { void chat.confirm(); }}>Коды совпадают</button>
          {view.peerConfirmed && <p className="muted">Собеседник уже подтвердил код. Требуется ваше подтверждение.</p>}
          <button className="secondary" onClick={() => chat.close('Коды не совпали. Соединение закрыто, переписка удалена.')}>Коды не совпадают</button>
        </> : <p role="status" className="muted">Вы подтвердили код. Переписка откроется после подтверждения второй стороны.</p>}
      </div>}
      {remaining !== null && <div className="deadline">
        <span>Осталось <time aria-label="Оставшееся время">{Math.floor(remaining / 60).toString().padStart(2, '0')}:{(remaining % 60).toString().padStart(2, '0')}</time></span>
        {remaining <= 60 ? <strong role="status">До завершения чата меньше минуты.</strong> : remaining <= 300 ? <strong role="status">До завершения чата меньше пяти минут.</strong> : null}
      </div>}

      {active && <button className="exit" onClick={() => chat.leave()}>Выйти и завершить чат</button>}
      {view.phase === 'closed' && <div className="stack"><p>Этот разговор нельзя восстановить. Для следующего подключения понадобится новая ссылка.</p><a className="button primary" href="/">Создать новый чат</a></div>}
    </section>
    {help && <HelpDialog onClose={() => setHelp(false)} capture={{ platform: 'browser', enabled: false, limitation: 'Браузер не может запретить все способы записи экрана.' }} />}
    <footer><p>Сообщения шифруются на устройствах. Сервер помогает установить соединение и при необходимости передаёт зашифрованный трафик.</p><p>Сверка кода обязательна. Эта страница не защищает от доступа к самому устройству.</p><p>Веб-версия требует доверия к серверу, который отдаёт код страницы. Сохранённые собеседником копии она удалить не может.</p></footer>
  </main>;
}

export default function App() { return <ThemeProvider><WebContent /></ThemeProvider>; }
