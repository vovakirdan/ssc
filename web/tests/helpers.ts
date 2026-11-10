import { expect, type Browser, type BrowserContext, type Page } from '@playwright/test';
export async function peerContext(browser: Browser): Promise<BrowserContext> {
  const context = await browser.newContext();
  if (process.env.SSC_WEB_FORCE_RELAY === '1') await context.addInitScript(() => {
    const peers: RTCPeerConnection[] = [];
    const Native = window.RTCPeerConnection;
    window.RTCPeerConnection = class extends Native {
      constructor(configuration?: RTCConfiguration) { super({ ...configuration, iceTransportPolicy: 'relay' }); peers.push(this); }
    };
    Object.defineProperty(window, '__sscRelayStats', { value: async () => Promise.all(peers.map(async peer => {
      const stats = await peer.getStats();
      let pair: RTCStats | undefined;
      stats.forEach(value => { if (value.type === 'transport' && value.selectedCandidatePairId) pair = stats.get(value.selectedCandidatePairId); });
      if (!pair) stats.forEach(value => { if (value.type === 'candidate-pair' && value.nominated && value.state === 'succeeded') pair = value; });
      const selected = pair as RTCStats & { localCandidateId?: string; remoteCandidateId?: string } | undefined;
      return { ice: peer.iceConnectionState, local: selected?.localCandidateId ? stats.get(selected.localCandidateId)?.candidateType : null, remote: selected?.remoteCandidateId ? stats.get(selected.remoteCandidateId)?.candidateType : null };
    })) });
  });
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
export async function confirm(creator: Page, guest: Page) {
  const a = creator.getByLabel('Код сверки'), b = guest.getByLabel('Код сверки');
  await expect(a).toBeVisible(); await expect(b).toBeVisible();
  const matched = (await a.textContent()) === (await b.textContent());
  expect(matched, 'Both peers must have the same SAS').toBe(true);
  await expect(creator.getByRole('button', { name: 'Коды совпадают', exact: true })).toBeDisabled();
  await creator.getByRole('checkbox', { name: 'Я сверил(а) весь код с собеседником' }).check();
  await creator.getByRole('button', { name: 'Коды совпадают', exact: true }).click();
  await expect(creator.getByRole('heading', { name: 'Ожидаем подтверждения собеседника…' })).toBeVisible();
  await expect(creator.getByLabel('Сообщение', { exact: true })).toHaveCount(0);
  await guest.getByRole('checkbox', { name: 'Я сверил(а) весь код с собеседником' }).check();
  await guest.getByRole('button', { name: 'Коды совпадают', exact: true }).click();
  await expect(creator.getByLabel('Сообщение', { exact: true })).toBeEnabled(); await expect(guest.getByLabel('Сообщение', { exact: true })).toBeEnabled();
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
