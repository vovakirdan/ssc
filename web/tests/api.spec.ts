// HTTP boundary mocks only. Room/WebRTC/crypto acceptance is in rooms.spec.ts.
import { test, expect, type Page } from '@playwright/test';

const id = 'A'.repeat(43), token = 'B'.repeat(42) + 'A';
async function stateResult(page: Page) {
  return page.evaluate(async access => {
    const path = '/src/api.ts';
    const module = await import(/* @vite-ignore */ path);
    try {
      const value = await new module.RoomsApi(new AbortController().signal).state(access);
      return { ok: true, value, code: null, message: null };
    } catch (error) {
      return { ok: false, value: null, code: error instanceof module.RoomError ? error.code : 'unexpected', message: error instanceof Error ? error.message : 'unexpected' };
    }
  }, { id, token });
}
async function response(page: Page, status: number, body: string | Buffer, contentType = 'application/json') {
  await page.route(`**/v1/rooms/${id}`, route => route.fulfill({ status, body, contentType }));
  await page.goto('/');
  return stateResult(page);
}

for (const [status, code] of [[429, 'rate_limited'], [503, 'capacity'], [502, 'network'], [504, 'network']] as const) {
  test(`HTML HTTP ${status} remains retryable as ${code}`, async ({ page }) => {
    // An oversized malformed proxy body must never override the known HTTP status.
    const body = `<html>private proxy diagnostic${status === 429 ? 'x'.repeat(2 * 1024 * 1024) : ''}</html>`;
    const result = await response(page, status, body, 'text/html');
    expect(result).toMatchObject({ ok: false, code, message: 'Room operation failed' });
    expect(result.message).not.toContain('private proxy diagnostic');
  });
}

test('malformed success remains fatal and the successful body limit is preserved', async ({ page }) => {
  const cases = [
    { body: '<html>not an API response</html>', type: 'text/html' },
    { body: Buffer.from([0xff]), type: 'application/json' },
    { body: JSON.stringify({ padding: 'x'.repeat(1024 * 1024) }), type: 'application/json' },
  ];
  for (const value of cases) {
    await page.unrouteAll();
    const result = await response(page, 200, value.body, value.type);
    expect(result).toMatchObject({ ok: false, code: 'invalid_response' });
  }
});

test('successful JSON still requires valid room metadata', async ({ page }) => {
  const valid = { id, state: 'waiting', join_expires_at: new Date(Date.now() + 300_000).toISOString(), expires_at: null };
  for (const body of [{}, { ...valid, id: 'wrong' }, { ...valid, state: 'connected' }, { ...valid, join_expires_at: 'wrong' }]) {
    await page.unrouteAll();
    expect(await response(page, 200, JSON.stringify(body))).toMatchObject({ ok: false, code: 'invalid_response' });
  }
  await page.unrouteAll();
  expect(await response(page, 200, JSON.stringify(valid))).toMatchObject({ ok: true, value: valid });
});

test('bad-request bodies do not become retryable proxy errors', async ({ page }) => {
  expect(await response(page, 400, '<html>bad request</html>', 'text/html')).toMatchObject({ ok: false, code: 'invalid_response' });
  await page.unrouteAll();
  expect(await response(page, 400, JSON.stringify({ error: 'invalid_request' }))).toMatchObject({ ok: false, code: 'request_failed' });
});

async function endRoom(page: Page, keepalive = false) {
  return page.evaluate(async ({ access, keepalive }) => {
    const path = '/src/api.ts';
    const module = await import(/* @vite-ignore */ path);
    const retired = new AbortController(); retired.abort();
    await new module.RoomsApi(retired.signal).end(access, keepalive);
  }, { access: { id, token }, keepalive });
}

test('normal cleanup retries HTML429 with the exact same capability after session retirement', async ({ page }) => {
  const calls: { method: string; path: string; authorization: string | undefined }[] = [];
  await page.route(`**/v1/rooms/${id}`, route => {
    const request = route.request();
    calls.push({ method: request.method(), path: new URL(request.url()).pathname, authorization: request.headers().authorization });
    return route.fulfill({ status: calls.length === 1 ? 429 : 200, contentType: 'text/html', body: calls.length === 1 ? '<html>proxy limit</html>' : '{}' });
  });
  await page.goto('/'); await endRoom(page);
  expect(calls).toEqual(Array.from({ length: 2 }, () => ({ method: 'DELETE', path: `/v1/rooms/${id}`, authorization: `Bearer ${token}` })));
});

test('keepalive cleanup makes one immediate request even on HTML429', async ({ page }) => {
  let calls = 0;
  await page.route(`**/v1/rooms/${id}`, route => { calls++; return route.fulfill({ status: 429, contentType: 'text/html', body: '<html>proxy limit</html>' }); });
  await page.goto('/'); await endRoom(page, true);
  expect(calls).toBe(1);
});

test('successful, already-closed and permanent client cleanup responses never retry', async ({ page }) => {
  await page.goto('/');
  for (const status of [200, 204, 404, 401, 400]) {
    let calls = 0; await page.unrouteAll();
    await page.route(`**/v1/rooms/${id}`, route => { calls++; return route.fulfill({ status, contentType: 'text/html', body: status === 204 ? '' : '<html>terminal response</html>' }); });
    await endRoom(page);
    expect(calls).toBe(1);
  }
});

test('normal cleanup stops after at most three network failures', async ({ page }) => {
  let calls = 0;
  await page.route(`**/v1/rooms/${id}`, route => { calls++; return route.abort(); });
  await page.goto('/'); await endRoom(page);
  expect(calls).toBe(3);
});

test('normal cleanup aborts a stalled request at the independent five-second deadline', async ({ page }) => {
  await page.clock.install({ time: new Date() });
  let calls = 0;
  await page.route(`**/v1/rooms/${id}`, () => { calls++; });
  await page.goto('/');
  const cleanup = endRoom(page);
  await expect.poll(() => calls).toBe(1);
  await page.clock.fastForward(5001);
  await cleanup;
  expect(calls).toBe(1);
});
