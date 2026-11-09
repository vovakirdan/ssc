import { test, expect } from '@playwright/test';
test('browser runs the shared WASM commit/reveal, readiness and UTF-8 chunks', async ({ page }) => {
  await page.goto('/');
  const modulePath = process.env.SSC_PROTOCOL_TEST_URL || '/protocol/ssc_protocol.js';
  const result = await page.evaluate(async (moduleUrl) => {
    const { default: initialize, ProtocolSession } = await import(/* @vite-ignore */ moduleUrl);
    await initialize();
    const random = (size: number) => crypto.getRandomValues(new Uint8Array(size));
    const a = new ProtocolSession('0123456789abcdef0123456789abcdef', true, random(32), random(32));
    const b = new ProtocolSession('0123456789abcdef0123456789abcdef', false, random(32), random(32));
    try {
      const queue: { target: typeof a; frame: Uint8Array }[] = [];
      for (const frame of a.start()) queue.push({ target: b, frame });
      for (const frame of b.start()) queue.push({ target: a, frame });
      for (let i = 0; queue.length && i < 20; i++) {
        const { target, frame } = queue.shift()!;
        for (const outgoing of target.receive(frame).outgoing) queue.push({ target: target === a ? b : a, frame: outgoing });
      }
      const matched = !!a.sas() && a.sas() === b.sas();
      let blocked = false;
      try { a.send_text('blocked', random(16)); } catch { blocked = true; }
      b.receive(a.confirm()); a.receive(b.confirm());
      const message = '🔒 Привет из браузера\n'.repeat(1500);
      let received: string | null = null;
      for (const frame of a.send_text(message, random(16))) received = b.receive(frame).message ?? received;
      return { matched, blocked, ready: a.local_ready() && a.peer_ready() && b.local_ready() && b.peer_ready(), delivered: received === message };
    } finally { a.close(); a.free(); b.close(); b.free(); }
  }, modulePath);
  expect(result).toEqual({ matched: true, blocked: true, ready: true, delivered: true });
});
