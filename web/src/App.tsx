import { useEffect, useState, type FormEvent } from 'react';
import { BrowserChat, type ChatView } from './chat';

const labels = {
  home: 'Приватный разговор по одной ссылке', preview: 'Вас пригласили в приватный чат', working: 'Готовим защищённое соединение…',
  waiting: 'Приглашение готово', connecting: 'Соединяемся с собеседником…', verification: 'Сверьте код с собеседником',
  waiting_peer: 'Ожидаем подтверждения собеседника…', chat: 'Защищённый чат', recovering: 'Восстанавливаем связь…', closed: 'Чат завершён',
};
function Conversation({ chat, view }: { chat: BrowserChat; view: ChatView }) {
  const [text, setText] = useState('');
  const [sending, setSending] = useState(false);
  const send = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault(); if (sending || !text.trim()) return;
    setSending(true); const message = text; setText('');
    try { await chat.send(message); } finally { setSending(false); }
  };
  return <>
    <ol className="messages" aria-label="Переписка" aria-live="polite" aria-relevant="additions">
      {view.messages.map(message => <li key={message.id} className={message.own ? 'message own' : 'message'}>
        <span className="message-author">{message.own ? 'Вы · передано каналу' : 'Собеседник'}</span><p>{message.text}</p>
      </li>)}
      {view.messages.length === 0 && <li className="muted">Сообщения остаются только в памяти этой страницы.</li>}
    </ol>
    <form onSubmit={event => { void send(event); }} className="composer">
      <label className="visually-hidden" htmlFor="message">Сообщение</label>
      <textarea id="message" aria-label="Сообщение" value={text} onChange={event => setText(event.target.value)} rows={2} maxLength={256 * 1024}
        disabled={sending || view.phase !== 'chat'} placeholder="Напишите сообщение…" />
      <button type="submit" disabled={sending || view.phase !== 'chat' || !text.trim()}>Отправить</button>
    </form>
    <p className="fine">«Передано каналу» означает передачу вашему соединению, а не подтверждение прочтения.</p>
  </>;
}
export default function App() {
  const [chat] = useState(() => new BrowserChat());
  const [view, setView] = useState(() => chat.snapshot());
  const [compared, setCompared] = useState(false);
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState(false);
  useEffect(() => {
    const unsubscribe = chat.subscribe(setView);
    const pagehide = () => chat.close(null, true);
    window.addEventListener('pagehide', pagehide);
    return () => { unsubscribe(); window.removeEventListener('pagehide', pagehide); chat.close(null, true); };
  }, [chat]);
  const remaining = view.remainingSeconds;
  const copy = async () => {
    try { await navigator.clipboard.writeText(view.invite); setCopied(true); setCopyError(false); }
    catch { setCopyError(true); }
  };
  const active = !['home', 'preview', 'closed'].includes(view.phase);
  const conversation = ['chat', 'recovering'].includes(view.phase);
  return <main className="shell">
    <header className="brand"><span className="brand-icon" aria-hidden="true">◈</span><div><p>SECRET CHAT</p><span>Без регистрации · один собеседник</span></div></header>
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
      {conversation && <Conversation chat={chat} view={view} />}
      {active && <button className="exit" onClick={() => chat.close('Вы вышли. Разговор завершён, его историю нельзя восстановить в приложении.')}>Выйти и завершить чат</button>}
      {view.phase === 'closed' && <div className="stack"><p>Этот разговор нельзя восстановить. Для следующего подключения понадобится новая ссылка.</p><a className="button primary" href="/">Создать новый чат</a></div>}
    </section>
    <footer><p>Сообщения шифруются на устройствах. Сервер помогает установить соединение и при необходимости передаёт зашифрованный трафик.</p><p>Сверка кода обязательна. Эта страница не защищает от доступа к самому устройству.</p><p>Веб-версия требует доверия к серверу, который отдаёт код страницы. Сохранённые собеседником копии она удалить не может.</p></footer>
  </main>;
}
