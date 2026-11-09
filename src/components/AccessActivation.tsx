import { useEffect, useRef, useState, type FormEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { useToast } from '@/hooks/use-toast';
import type { AccessStatus } from '@/lib/preferences';

// Show only known local messages, never echo activation data from an IPC failure.
function activationError(failure: unknown): string {
  const code = typeof failure === 'object' && failure !== null && 'code' in failure ? failure.code : undefined;
  switch (code) {
    case 'access_code_invalid': return 'Проверьте код: нужны 43 символа латиницы, цифры, «-» или «_».';
    case 'access_already_active': return 'Установка уже активирована. Для замены доступа сначала сбросьте локальную активацию.';
    case 'access_denied': return 'Код просрочен, использован другой установкой или доступ отозван. Запросите новый код.';
    case 'access_rate_limited': return 'Слишком много попыток. Подождите и повторите активацию.';
    case 'access_busy': return 'Профиль занят другим процессом. Повторите активацию позже.';
    case 'access_storage': return 'Не удалось сохранить доступ на этом устройстве. Проверьте доступность папки приложения.';
    default: return 'Не удалось выполнить активацию. Проверьте подключение и повторите попытку.';
  }
}

export default function AccessActivation() {
  const [status, setStatus] = useState<AccessStatus | null>(null);
  const [code, setCode] = useState('');
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [resetOpen, setResetOpen] = useState(false);
  const mounted = useRef(false);
  const { toast } = useToast();
  const active = status?.state === 'active';
  const reference = active ? status.access_reference : undefined;

  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    void invoke<AccessStatus>('get_access_status').then(value => {
      if (!disposed) setStatus(value);
    }).catch(() => {
      if (!disposed) setStatus({ state: 'error' });
    }).finally(() => { if (!disposed) setBusy(false); });
    return () => { disposed = true; mounted.current = false; };
  }, []);

  const perform = async (command: 'activate_access' | 'initialize_access' | 'reset_access') => {
    if (busy) return;
    const token = code.trim();
    if (command === 'activate_access' && !/^[A-Za-z0-9_-]{43}$/.test(token)) {
      setError(activationError({ code: 'access_code_invalid' }));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const next = command === 'reset_access'
        ? (await invoke(command), { state: 'inactive' } as const)
        : await invoke<AccessStatus>(command, command === 'activate_access' ? { code: token } : undefined);
      if (!mounted.current) return;
      setStatus(next);
      if (next.state === 'active' || command === 'reset_access') setCode('');
      if (next.state === 'error') setError(activationError(next.error));
      if (command === 'reset_access') setResetOpen(false);
    } catch (failure) {
      if (mounted.current) setError(activationError(failure));
    } finally { if (mounted.current) setBusy(false); }
  };
  const activate = (event: FormEvent) => { event.preventDefault(); void perform('activate_access'); };
  const copyReference = async () => {
    if (!reference) return;
    try { await navigator.clipboard.writeText(reference); toast({ title: 'Номер доступа скопирован' }); }
    catch { toast({ title: 'Не удалось скопировать', description: 'Выделите номер доступа и скопируйте его вручную.', variant: 'destructive' }); }
  };

  return <section aria-label="Доступ к TURN проекта" className="space-y-3 rounded-lg border border-slate-600 p-3 text-sm text-slate-300">
    <h3 className="font-medium text-slate-100">Доступ к TURN проекта</h3>
    <p role="status">{status === null ? 'Проверяем доступ…' : active ? 'Установка активирована. Доступ проверяется при подключении.' : status.state === 'error' ? 'Не удалось проверить активацию.' : 'Установка не активирована. В автоматическом режиме используется прямое соединение со STUN.'}</p>
    {!active && <form aria-label="Активация TURN проекта" onSubmit={activate} className="space-y-2">
      <Label htmlFor="activation-code">Код активации</Label>
      <Input id="activation-code" aria-label="Код активации" type="password" autoComplete="off" autoCapitalize="none" spellCheck={false}
        value={code} onChange={event => { setCode(event.target.value); setError(null); }} disabled={busy}
        aria-describedby="activation-code-help" className="font-mono" />
      <p id="activation-code-help" className="text-xs text-slate-400">Получите одноразовый код у владельца сервиса. Активация не меняет выбранный способ подключения и действует для следующих соединений.</p>
      <Button type="submit" disabled={busy || !code.trim()}>{busy ? 'Активируем…' : 'Активировать'}</Button>
    </form>}
    {error && <p role="alert" className="text-red-300">{error}</p>}
    {reference && <div className="space-y-2">
      <Label htmlFor="access-reference">Номер доступа</Label>
      <Input id="access-reference" aria-label="Номер доступа" value={reference} readOnly className="font-mono text-xs" />
      <p className="text-xs text-slate-400">Передайте этот номер владельцу сервиса, если потребуется отозвать доступ установки. Это не код сверки собеседника.</p>
      <Button type="button" variant="secondary" onClick={() => { void copyReference(); }}>Скопировать номер доступа</Button>
    </div>}
    {!active && <Button type="button" variant="secondary" onClick={() => { void perform('initialize_access'); }} disabled={busy}>Повторить активацию</Button>}
    <Dialog open={resetOpen} onOpenChange={setResetOpen}>
      <DialogTrigger asChild><Button type="button" variant="ghost" disabled={busy}>Сбросить локальную активацию</Button></DialogTrigger>
      <DialogContent>
        <DialogHeader><DialogTitle>Удалить локальный допуск?</DialogTitle>
          <DialogDescription>Ключ и данные активации на этом компьютере будут удалены. Для повторной активации понадобится новый код или персональный установщик. Другие копии ключа и допуск на сервере этим действием не отзываются. Свои TURN-серверы останутся в настройках.</DialogDescription>
        </DialogHeader>
        <Button type="button" variant="destructive" disabled={busy} onClick={() => { void perform('reset_access'); }}>Удалить локальный допуск</Button>
      </DialogContent>
    </Dialog>
  </section>;
}
