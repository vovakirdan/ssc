// Two native Tauri windows and the real, separately built ssc-connect service.
import { resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { request, execute, ipc, until, click, input, visible } from './webdriver.mjs';

const origin = process.env.SSC_CONNECT_URL || 'http://127.0.0.1:8080';
const base = new URL(origin);
if (base.protocol !== 'http:' || !['localhost', '127.0.0.1', '[::1]'].includes(base.hostname)) {
  throw new Error('Native smoke requires a local HTTP service; use the same SSC_CONNECT_URL for both drivers');
}
let service;
let serviceExit;
let serviceLog = '';
const sessions = [];
const secrets = [];
async function startService() {
  const probe = createServer();
  await new Promise((resolve, reject) => { probe.once('error', reject); probe.listen(Number(base.port || 80), base.hostname.replaceAll(/[\[\]]/g, ''), resolve); });
  await new Promise(resolve => probe.close(resolve));
  service = spawn(process.env.SSC_CONNECT_BINARY || resolve('../ssc-connect/bin/ssc-connect'), [], {
    env: { ...process.env, SSC_LISTEN_ADDR: base.host, SSC_PUBLIC_URL: origin }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  serviceExit = once(service, 'exit');
  service.stdout.on('data', data => { serviceLog += data; });
  service.stderr.on('data', data => { serviceLog += data; });
  await until(async () => {
    if (service.exitCode !== null) throw new Error('Local test service exited before becoming ready');
    try { return (await fetch(origin + '/healthz')).status === 200; } catch { return false; }
  }, 'local service readiness');
}
async function stopService() {
  if (!service) return;
  service.kill('SIGINT');
  let timer;
  try { await Promise.race([serviceExit, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Test service did not stop')), 7000); })]); }
  catch (failure) { service.kill('SIGKILL'); await serviceExit; throw failure; }
  finally { clearTimeout(timer); service = null; }
}
function byLabel(peer,label){return execute(peer,'return document.querySelector(arguments[0])?.value ?? null', [`[aria-label="${label}"]`]);}
function hasLabel(peer,label){return execute(peer,'return !!document.querySelector(arguments[0])', [`[aria-label="${label}"]`]);}
function clickLabel(peer,label){return execute(peer,'document.querySelector(arguments[0]).click()', [`[aria-label="${label}"]`]);}
async function openLinkPage(peer) {
  if (await visible(peer, 'Начать чат')) await click(peer, 'Начать чат');
  await clickLabel(peer, 'Подключиться по короткой ссылке');
  await until(() => hasLabel(peer, 'Ссылка приглашения'), 'link screen');
}
async function createLink(peer) {
  await click(peer, 'Создать ссылку');
  await until(() => byLabel(peer, 'Короткая ссылка'), 'short invitation');
  const url = await byLabel(peer, 'Короткая ссылка');
  secrets.push(url, new URL(url).pathname.split('/').at(-1));
  return url;
}
async function confirmPair() {
  for (const peer of sessions) await until(() => visible(peer, 'Я вижу одинаковый код'), 'SAS verification');
  const verification = await ipc(sessions[0], 'get_fingerprint');
  const sas = verification?.sas;
  if (!sas || sas !== (await ipc(sessions[1], 'get_fingerprint'))?.sas) throw new Error('SAS mismatch');
  secrets.push(sas);
  await execute(sessions[0], 'document.querySelector("input[type=checkbox]").click()');
  await click(sessions[0], 'Коды совпадают');
  await until(() => visible(sessions[0], 'Ожидаем подтверждения собеседника'), 'one-sided confirmation wait');
  await execute(sessions[1], 'document.querySelector("input[type=checkbox]").click()');
  await click(sessions[1], 'Коды совпадают');
  for (const peer of sessions) await until(() => hasLabel(peer, 'Сообщение'), 'chat');
}
async function send(sender, receiver, text) {
  secrets.push(text);
  await input(sender, '[aria-label="Сообщение"]', text);
  await clickLabel(sender, 'Отправить');
  await until(() => visible(receiver, text), 'chat message');
}
try {
  await startService();
  for (const port of [4444, 4445]) {
    const result = await request(`http://127.0.0.1:${port}/session`, { capabilities: { alwaysMatch: { 'tauri:options': { application: process.env.SSC_BINARY || resolve('src-tauri/target/release/ssc') } } } });
    sessions.push({ url: `http://127.0.0.1:${port}/session/${result.sessionId}` });
  }
  for (const peer of sessions) {
    await until(() => visible(peer, 'Начать чат'), 'welcome');
    await ipc(peer, 'set_ice_servers', { servers: [] });
    await ipc(peer, 'set_diagnostics', { enabled: true });
    await openLinkPage(peer);
  }
  const invitation = await createLink(sessions[0]);
  await input(sessions[1], '[aria-label="Ссылка приглашения"]', invitation);
  await click(sessions[1], 'Подключиться по ссылке');
  await confirmPair();
  await stopService();
  await send(sessions[0], sessions[1], 'native short-link hello 🦀');
  await send(sessions[1], sessions[0], 'native short-link reply 🦀');
  console.log('PASS real service + pasted link + automatic answer + mandatory SAS');
  console.log('PASS bidirectional chat with the helper stopped');
  for (const peer of sessions) {
    const report = JSON.stringify(await ipc(peer, 'get_diagnostics'));
    if (secrets.some(secret => report.includes(secret))) throw new Error('Sensitive data in diagnostic report');
    if ((await ipc(peer, 'get_link_state'))?.phase !== 'complete') throw new Error('Helper completion state lost');
  }
  console.log('PASS diagnostic redaction and completed state');
  for (const peer of sessions) await clickLabel(peer, 'Выйти из чата');
  await startService();
  await openLinkPage(sessions[0]);
  const stale = await createLink(sessions[0]);
  await stopService();
  await until(() => visible(sessions[0], 'Повторить подключение'), 'retryable helper outage');
  if ((await byLabel(sessions[0], 'Короткая ссылка')) !== stale) throw new Error('Outage silently replaced the invitation');
  await startService();
  await click(sessions[0], 'Повторить подключение');
  await until(async () => (await ipc(sessions[0], 'get_link_state'))?.error?.retryable === false, 'lost invitation after server restart');
  if ((await ipc(sessions[0], 'get_link_state')).url !== stale) throw new Error('Restart silently replaced invitation');
  console.log('PASS outage/retry/restart failure without automatic new invitation');
} finally {
  for (const peer of sessions) await request(peer.url, undefined, 'DELETE');
  await stopService();
  if (secrets.some(secret => serviceLog.includes(secret))) throw new Error('Sensitive data in service log');
}
