import { ArrowLeft, ArrowRight, Link2, QrCode, ScanLine, ShieldCheck } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface Props { onCreate: () => void; onJoin: () => void; onBack: () => void; onLink: () => void; onTemporary?: () => void }

export default function ConnectionHub({ onCreate, onJoin, onBack, onLink, onTemporary }: Props) {
  return <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-800 text-white p-5 sm:p-8">
    <main className="max-w-2xl mx-auto space-y-7">
      <Button variant="ghost" onClick={onBack} className="text-slate-300 -ml-3"><ArrowLeft className="w-4 h-4 mr-2" />Назад</Button>
      <header className="space-y-3">
        <p className="text-emerald-400 text-xs uppercase tracking-[0.2em]">Новое подключение</p>
        <h1 className="text-3xl sm:text-4xl font-semibold tracking-tight">Пригласите своего человека</h1>
        <p className="text-slate-400 leading-relaxed">Поделитесь короткой ссылкой или обменяйтесь кодами вручную. Перед началом чата вы сверите код на обоих устройствах.</p>
      </header>
      <button onClick={onLink} aria-label="Подключиться по короткой ссылке" className="w-full rounded-2xl border border-cyan-800/60 bg-cyan-950/20 hover:bg-cyan-950/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-400 p-5 flex items-start gap-4 text-left transition-colors">
        <Link2 className="w-5 h-5 text-cyan-400 mt-1 flex-shrink-0" />
        <div className="flex-1"><h2 className="text-slate-100 font-medium">По короткой ссылке</h2><p className="text-sm text-slate-400 mt-1">Создайте ссылку или вставьте полученную. Приложения обменяются ответом автоматически.</p></div>
        <ArrowRight className="w-5 h-5 text-cyan-400 mt-1 flex-shrink-0" />
      </button>
      {onTemporary && <button type="button" onClick={onTemporary} aria-label="Временный чат без установки" className="w-full rounded-2xl border border-emerald-700 bg-emerald-950/30 p-5 text-left hover:bg-emerald-950/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"><h2 className="font-medium">Временный чат без установки</h2><p className="mt-1 text-sm text-slate-300">Собеседник откроет ссылку в браузере. Вход — в течение 5 минут, разговор — до часа.</p></button>}
      <p className="text-sm text-slate-400">Или вручную — QR и коды</p>
      <div className="grid sm:grid-cols-2 gap-4">
        <button onClick={onCreate} className="group text-left rounded-2xl border border-emerald-700/60 bg-emerald-950/30 hover:bg-emerald-950/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400 p-6 transition-colors">
          <QrCode className="w-7 h-7 text-emerald-400 mb-6" />
          <p className="text-xs text-emerald-300 mb-2">Я приглашаю</p>
          <h2 className="text-lg font-semibold mb-2">Создать приглашение</h2>
          <p className="text-slate-400 text-sm leading-relaxed">Получите QR и код приглашения, чтобы передать их собеседнику.</p>
          <ArrowRight className="w-5 h-5 mt-5 text-emerald-400 group-hover:translate-x-1 transition-transform" />
        </button>
        <button onClick={onJoin} className="group text-left rounded-2xl border border-slate-700 bg-slate-800/40 hover:bg-slate-800/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400 p-6 transition-colors">
          <ScanLine className="w-7 h-7 text-slate-300 mb-6" />
          <p className="text-xs text-slate-400 mb-2">Меня пригласили</p>
          <h2 className="text-lg font-semibold mb-2">Принять приглашение</h2>
          <p className="text-slate-400 text-sm leading-relaxed">Вставьте полученный код или загрузите изображение QR.</p>
          <ArrowRight className="w-5 h-5 mt-5 text-slate-400 group-hover:translate-x-1 transition-transform" />
        </button>
      </div>
      <p className="flex gap-2 text-xs text-slate-500 leading-relaxed"><ShieldCheck className="w-4 h-4 flex-shrink-0" />Передавайте приглашение и ответ только тому человеку, с которым хотите общаться.</p>
    </main>
  </div>;
}
