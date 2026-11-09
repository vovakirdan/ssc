import { test, expect, type Browser, type BrowserContext, type Page } from '@playwright/test';

async function peerContext(browser: Browser): Promise<BrowserContext> {
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

async function openInvitation(page: Page, invitation: string) {
  // Pass the bearer fragment inside the browser, never a Playwright navigation log.
  await page.goto('/');
  const loaded = page.waitForEvent('domcontentloaded');
  await page.evaluate(value => { location.replace(value); }, invitation);
  await loaded;
  await expect(page.getByRole('button', { name: 'Войти', exact: true })).toBeVisible();
}
async function confirm(creator: Page, guest: Page) {
  const a = creator.getByLabel('Код сверки'), b = guest.getByLabel('Код сверки');
  await expect(a).toBeVisible(); await expect(b).toBeVisible();
  const matched = (await a.textContent()) === (await b.textContent());
  expect(matched, 'Both peers must have the same SAS').toBe(true);
  await expect(creator.getByRole('button', { name: 'Коды совпадают', exact: true })).toBeDisabled();
  await creator.getByRole('checkbox', { name: 'Я сверил(а) весь код с собеседником' }).check();
  await creator.getByRole('button', { name: 'Коды совпадают', exact: true }).click();
  await expect(creator.getByRole('heading', { name: 'Ожидаем подтверждения собеседника…' })).toBeVisible();
  await expect(creator.getByLabel('Сообщение')).toHaveCount(0);
  await guest.getByRole('checkbox', { name: 'Я сверил(а) весь код с собеседником' }).check();
  await guest.getByRole('button', { name: 'Коды совпадают', exact: true }).click();
  await expect(creator.getByLabel('Сообщение')).toBeEnabled(); await expect(guest.getByLabel('Сообщение')).toBeEnabled();
}

test('preview does not claim; two browsers compare SAS, chat and erase on exit', async ({ browser }) => {
  const contexts: BrowserContext[] = [];
  try {
    const creatorContext = await peerContext(browser), previewContext = await browser.newContext(), guestContext = await peerContext(browser);
    contexts.push(creatorContext, previewContext, guestContext);
    const creator = await creatorContext.newPage(), preview = await previewContext.newPage(), guest = await guestContext.newPage();
    await creator.clock.install({ time: new Date() });
    await creator.goto('/');
    await creator.getByRole('button', { name: 'Создать чат', exact: true }).click();
    const invitationField = creator.getByLabel('Ссылка приглашения');
    await expect(invitationField).toBeVisible();
    const invitation = await invitationField.inputValue();
    expect(invitation.length > 80).toBe(true);
    await openInvitation(preview, invitation);
    await preview.close();
    await openInvitation(guest, invitation);
    await guest.getByRole('button', { name: 'Войти', exact: true }).click();
    await confirm(creator, guest);
    if (process.env.SSC_WEB_FORCE_RELAY === '1') for (const page of [creator, guest]) {
      const relay = await page.evaluate(async () => {
        const read = (window as unknown as { __sscRelayStats: () => Promise<{ ice: string; local: string | null; remote: string | null }[]> }).__sscRelayStats;
        return (await read()).every(pair => pair.local === 'relay' && pair.remote === 'relay');
      });
      expect(relay, 'Actual selected candidate pair must use relay on both sides').toBe(true);
    }
    await creator.getByLabel('Сообщение').fill('browser greeting 🦀');
    await creator.getByRole('button', { name: 'Отправить', exact: true }).click();
    await expect(guest.getByText('browser greeting 🦀', { exact: true })).toBeVisible();
    await guest.getByLabel('Сообщение').fill('browser reply 🦀');
    await guest.getByRole('button', { name: 'Отправить', exact: true }).click();
    await expect(creator.getByText('browser reply 🦀', { exact: true })).toBeVisible();
    for (const page of [creator, guest]) {
      const storageEmpty = await page.evaluate(() => localStorage.length === 0 && sessionStorage.length === 0 && location.hash === '');
      expect(storageEmpty).toBe(true);
    }
    const countdown = creator.getByLabel('Оставшееся время');
    const seconds = (value: string | null) => { const parts = (value || '').split(':').map(Number); return parts[0] * 60 + parts[1]; };
    const beforeRollback = seconds(await countdown.textContent());
    await creator.clock.setSystemTime(new Date(Date.now() - 3 * 3600_000));
    await creator.clock.runFor(1100);
    await expect(creator.getByRole('heading', { name: 'Защищённый чат' })).toBeVisible();
    expect(seconds(await countdown.textContent())).toBeLessThanOrEqual(beforeRollback);
    const polled = creator.waitForResponse(response => response.request().method() === 'GET' && new URL(response.url()).pathname.startsWith('/v1/rooms/'));
    await creator.clock.setSystemTime(new Date(Date.now() + 3 * 3600_000));
    await creator.clock.runFor(2100);
    await (await polled).finished();
    await creator.clock.runFor(100);
    await expect(creator.getByRole('heading', { name: 'Защищённый чат' })).toBeVisible();
    expect(seconds(await countdown.textContent())).toBeLessThanOrEqual(beforeRollback);
    expect(seconds(await countdown.textContent())).toBeGreaterThan(beforeRollback - 10);
    await creator.getByRole('button', { name: 'Выйти и завершить чат' }).click();
    await expect(creator.getByRole('heading', { name: 'Чат завершён' })).toBeVisible();
    await expect(guest.getByRole('heading', { name: 'Чат завершён' })).toBeVisible();
    await expect(creator.getByLabel('Переписка')).toHaveCount(0); await expect(guest.getByLabel('Переписка')).toHaveCount(0);
  } finally { for (const context of contexts) await context.close(); }
});

test('mobile preview is readable and makes no room API claim', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const roomRequests: string[] = [];
  page.on('request', request => { if (new URL(request.url()).pathname.startsWith('/v1/rooms')) roomRequests.push(request.method()); });
  // Synthetic capability, never submitted to the server.
  const placeholder = 'A'.repeat(43);
  await page.goto(`/r/${placeholder}#k=${placeholder}`);
  await expect(page.getByRole('heading', { name: 'Вас пригласили в приватный чат' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Войти', exact: true })).toBeEnabled();
  expect(roomRequests.length).toBe(0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth && location.hash === '' && localStorage.length === 0 && sessionStorage.length === 0)).toBe(true);
});


test('the monotonic one-hour deadline clears both peers even if the wall clock rolls back', async ({ browser }) => {
  const creatorContext = await peerContext(browser), guestContext = await peerContext(browser);
  try {
    const creator = await creatorContext.newPage(), guest = await guestContext.newPage();
    await creator.clock.install({ time: new Date() });
    await creator.goto('/');
    await creator.getByRole('button', { name: 'Создать чат', exact: true }).click();
    const invitationField = creator.getByLabel('Ссылка приглашения');
    await expect(invitationField).toBeVisible();
    await openInvitation(guest, await invitationField.inputValue());
    await guest.getByRole('button', { name: 'Войти', exact: true }).click();
    await confirm(creator, guest);
    await creator.clock.setSystemTime(new Date(Date.now() - 3 * 3600_000));
    await creator.clock.fastForward(3600_001);
    await expect(creator.getByRole('heading', { name: 'Чат завершён' })).toBeVisible();
    await expect(guest.getByRole('heading', { name: 'Чат завершён' })).toBeVisible();
    await expect(creator.getByLabel('Переписка')).toHaveCount(0);
    await expect(guest.getByLabel('Переписка')).toHaveCount(0);
  } finally { await creatorContext.close(); await guestContext.close(); }
});
