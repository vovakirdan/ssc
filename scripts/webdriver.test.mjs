import { test } from 'node:test';
import assert from 'node:assert/strict';
import { request, ipc } from './webdriver.mjs';

test('keeps structured native rejection codes available to IPC callers', async (t) => {
  t.mock.method(globalThis, 'fetch', async () => ({ json: async () => ({
    value: { ok: false, error: { code: 'verification_stale', message: 'Changed session' } },
  }) }));
  await assert.rejects(ipc({ url: 'http://driver.test/session/test' }, 'confirm_sas'), /verification_stale/);
});

test('rejects actual W3C driver protocol errors', async (t) => {
  t.mock.method(globalThis, 'fetch', async () => ({ json: async () => ({
    value: { error: 'invalid session id', message: 'No such session' },
  }) }));
  await assert.rejects(request('http://driver.test/session/test', {}), /invalid session id: No such session/);
});
