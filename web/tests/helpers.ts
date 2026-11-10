import { expect, type Browser, type BrowserContext, type Page } from '@playwright/test';
export async function peerContext(browser: Browser): Promise<BrowserContext> {
  const context = await browser.newContext();
  await context.addInitScript(forceRelay => {
    const peers: RTCPeerConnection[] = [];
    const Native = window.RTCPeerConnection;
    window.RTCPeerConnection = class extends Native {
      constructor(configuration?: RTCConfiguration) { super(forceRelay ? { ...configuration, iceTransportPolicy: 'relay' } : configuration); if (peers.length < 4) peers.push(this); }
    };
    Object.defineProperty(window, '__sscPeerState', { value: () => peers.map(peer => ({ connection: peer.connectionState, ice: peer.iceConnectionState, sctp: peer.sctp?.state ?? null })) });
    if (forceRelay) Object.defineProperty(window, '__sscRelayStats', { value: async () => Promise.all(peers.map(async peer => {
      const stats = await peer.getStats();
      let pair: RTCStats | undefined;
      stats.forEach(value => { if (value.type === 'transport' && value.selectedCandidatePairId) pair = stats.get(value.selectedCandidatePairId); });
      if (!pair) stats.forEach(value => { if (value.type === 'candidate-pair' && value.nominated && value.state === 'succeeded') pair = value; });
      const selected = pair as RTCStats & { localCandidateId?: string; remoteCandidateId?: string } | undefined;
      return { ice: peer.iceConnectionState, local: selected?.localCandidateId ? stats.get(selected.localCandidateId)?.candidateType : null, remote: selected?.remoteCandidateId ? stats.get(selected.remoteCandidateId)?.candidateType : null };
    })) });
  }, process.env.SSC_WEB_FORCE_RELAY === '1');
  return context;
}

export async function openInvitation(page: Page, invitation: string) {
  // Pass the bearer fragment inside the browser, never a Playwright navigation log.
  await page.goto('/');
  const loaded = page.waitForEvent('domcontentloaded');
  await page.evaluate(value => { location.replace(value); }, invitation);
  await loaded;
  await expect(page.getByRole('button', { name: 'Войти', exact: true })).toBeVisible();
}
type VerifyStep = 'creator_sas' | 'guest_sas' | 'compare' | 'creator_gate' | 'creator_confirm' | 'peer_wait' | 'guest_confirm' | 'chat_ready';
type CloseCode = 'none' | 'key_timeout' | 'connection_error' | 'channel_error' | 'crypto_error' | 'overflow' | 'service_error' | 'expired' | 'peer_exit' | 'other';
async function verificationSnapshot(page: Page) {
  try {
    return await page.evaluate(() => {
      const inspect = (window as unknown as { __sscPeerState?: () => unknown }).__sscPeerState;
      const values = typeof inspect === 'function' ? inspect() : [];
      const peers = (Array.isArray(values) ? values.slice(0, 4) : []).flatMap((value: unknown) => {
        if (!value || typeof value !== 'object') return [];
        const peer = value as Record<string, unknown>;
        const { connection, ice, sctp } = peer;
        if (typeof connection !== 'string' || !['new', 'connecting', 'connected', 'disconnected', 'failed', 'closed'].includes(connection) ||
          typeof ice !== 'string' || !['new', 'checking', 'connected', 'completed', 'disconnected', 'failed', 'closed'].includes(ice) ||
          (sctp !== null && (typeof sctp !== 'string' || !['connecting', 'connected', 'closed'].includes(sctp)))) return [];
        return [{ connection: connection as RTCPeerConnectionState, ice: ice as RTCIceConnectionState, sctp: sctp as RTCSctpTransportState | null }];
      });
      const notices = Array.from(document.querySelectorAll('[role="alert"], .ssc-closed > p:first-of-type')).map(node => node.textContent || '').filter(Boolean);
      const rules: ReadonlyArray<readonly [CloseCode, readonly string[]]> = [
        ['key_timeout', ['Обмен ключами не завершился.']],
        ['overflow', ['Входящая очередь переполнена.']],
        ['expired', ['Время чата истекло.']],
        ['peer_exit', ['Собеседник вышел.', 'Вы вышли.']],
        ['channel_error', ['Недопустимый канал соединения.', 'Ошибка защищённого канала.', 'Собеседник отключился.']],
        ['connection_error', ['Связь потеряна.', 'Соединение завершено.', 'Соединение прервано.']],
        ['crypto_error', ['Не удалось безопасно продолжить соединение.']],
        ['service_error', ['Чат закрыт или срок ссылки истёк.', 'Эта ссылка уже использована другим собеседником.', 'Создатель ещё готовит подключение.', 'Слишком много запросов.', 'Сервис временно занят.', 'Сервис недоступен.']],
      ];
      const closeCode: CloseCode = notices.length ? rules.find(([, prefixes]) => prefixes.some(prefix => notices.some(notice => notice.includes(prefix))))?.[0] || 'other' : 'none';
      return { hasSas: !!document.querySelector('[aria-label="Код сверки"]'), hasComposer: !!document.querySelector('[aria-label="Сообщение"]'), peers, closeCode };
    });
  } catch { return { hasSas: false, hasComposer: false, peers: [], closeCode: 'none' as const }; }
}
export async function confirm(creator: Page, guest: Page) {
  let step: VerifyStep = 'creator_sas';
  try {
    const a = creator.getByLabel('Код сверки'), b = guest.getByLabel('Код сверки');
    await expect(a).toBeVisible(); step = 'guest_sas'; await expect(b).toBeVisible();
    step = 'compare';
    const matched = (await a.textContent()) === (await b.textContent());
    expect(matched, 'Both peers must have the same SAS').toBe(true);
    step = 'creator_gate'; await expect(creator.getByRole('button', { name: 'Коды совпадают', exact: true })).toBeDisabled();
    step = 'creator_confirm';
    await creator.getByRole('checkbox', { name: 'Я сверил(а) весь код с собеседником' }).check();
    await creator.getByRole('button', { name: 'Коды совпадают', exact: true }).click();
    step = 'peer_wait';
    await expect(creator.getByRole('heading', { name: 'Ожидаем подтверждения собеседника…' })).toBeVisible();
    await expect(creator.getByLabel('Сообщение', { exact: true })).toHaveCount(0);
    step = 'guest_confirm';
    await guest.getByRole('checkbox', { name: 'Я сверил(а) весь код с собеседником' }).check();
    await guest.getByRole('button', { name: 'Коды совпадают', exact: true }).click();
    step = 'chat_ready';
    await expect(creator.getByLabel('Сообщение', { exact: true })).toBeEnabled(); await expect(guest.getByLabel('Сообщение', { exact: true })).toBeEnabled();
  } catch {
    const [creatorState, guestState] = await Promise.all([verificationSnapshot(creator), verificationSnapshot(guest)]);
    throw new Error('SSC_VERIFY_DIAG:' + JSON.stringify({ step, creator: creatorState, guest: guestState }));
  }
}


/** Capture only this page's newly-created capability in RAM; always await owned cleanup. */
export function trackOwnedRoom(page: Page): () => Promise<void> {
  let owned: { id: string; token: string } | undefined;
  const canonical = (value: unknown): value is string => typeof value === 'string' && /^[A-Za-z0-9_-]{43}$/.test(value);
  let pending: Promise<void> = Promise.resolve();
  page.on('response', response => {
    const url = new URL(response.url());
    if (url.pathname !== '/v1/rooms' || response.request().method() !== 'POST' || response.status() !== 201) return;
    pending = response.json().then(value => { if (canonical(value.id) && canonical(value.creator_token)) owned = { id: value.id, token: value.creator_token }; }).catch(() => undefined);
  });
  return async () => {
    await pending;
    if (!owned) return;
    const role = owned; owned = undefined;
    for (let attempt = 0; attempt < 3; attempt++) {
      try {
        const response = await fetch((process.env.SSC_WEB_TEST_URL || 'http://127.0.0.1:1430') + '/v1/rooms/' + role.id, { method: 'DELETE', headers: { authorization: 'Bearer ' + role.token }, redirect: 'error', signal: AbortSignal.timeout(3000) });
        await response.body?.cancel(); if ([200, 404].includes(response.status)) return;
      } catch { /* No raw URL/capability errors escape the owned cleanup. */ }
      await new Promise(resolve => setTimeout(resolve, 500));
    }
    throw new Error('Owned room cleanup failed; private details withheld');
  };
}

export async function leaveChat(page: Page) {
  const reveal = page.getByRole('button', { name: 'Показать чат', exact: true }); if (await reveal.isVisible()) await reveal.click();
  const wide = page.getByRole('button', { name: 'Выйти и завершить чат', exact: true });
  if (await wide.isVisible()) await wide.click(); else await page.getByRole('button', { name: 'Выйти из чата', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Завершить чат?' });
  if (await dialog.isVisible()) { await expect(dialog.getByRole('button', { name: 'Остаться', exact: true })).toBeFocused(); await dialog.getByRole('button', { name: 'Выйти и очистить', exact: true }).click(); }
}
