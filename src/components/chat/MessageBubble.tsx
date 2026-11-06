import type { Message } from './types';

export function MessageBubble({ msg }: { msg: Message }) {
  return <div className={`flex ${msg.isOwn ? 'justify-end' : 'justify-start'}`}>
    <div className={`max-w-[80%] rounded-xl p-3 ${msg.isOwn ? 'bg-emerald-700' : 'bg-slate-700'} text-white`}>
      <p className="text-sm whitespace-pre-wrap break-words">{msg.text}</p>
      <p className="text-xs mt-1 opacity-80">{msg.timestamp.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
        {msg.status === 'pending' && ' · Отправка…'}
        {msg.status === 'sent' && ' · Передано каналу'}
        {msg.status === 'failed' && ' · Не отправлено полностью'}
      </p>
    </div>
  </div>;
}
