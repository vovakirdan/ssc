// Run against two tauri-driver instances (4444/4445); no mocked IPC or transport.
import { resolve } from 'node:path';

const sessions = [];
import { request, execute, ipc, until, click, input, visible } from './webdriver.mjs';
async function connect([alice, bob]) {
  for (const peer of [alice, bob]) await click(peer, 'Начать чат');
  await click(alice, 'Создать приглашение');
  await until(() => execute(alice, 'return !!document.querySelector(arguments[0])?.value', ['[aria-label="Код приглашения"]']), 'offer screen');
  const offer = await execute(alice, 'return document.querySelector(arguments[0]).value', ['[aria-label="Код приглашения"]']);
  await click(bob, 'Принять приглашение');
  await input(bob, 'textarea', offer);
  await click(bob, 'Принять приглашение');
  await until(() => execute(bob, 'return !!document.querySelector(arguments[0])?.value', ['[aria-label="Код ответа"]']), 'answer screen');
  const answer = await execute(bob, 'return document.querySelector(arguments[0]).value', ['[aria-label="Код ответа"]']);
  await input(alice, '[placeholder="Вставьте ответ здесь..."]', answer);
  await click(alice, 'Подключиться');
  for (const peer of [alice, bob]) {
    await until(() => visible(peer, 'Я вижу одинаковый код'), 'verification UI');
  }
  const sas = await ipc(alice, 'get_fingerprint');
  if (!sas || sas !== await ipc(bob, 'get_fingerprint')) throw new Error('SAS mismatch');
  await execute(alice, 'document.querySelector("input[type=checkbox]").click()');
  await click(alice, 'Коды совпадают');
  await until(() => visible(alice, 'Ожидаем подтверждения собеседника'), 'one-sided confirmation');
  await execute(bob, 'document.querySelector("input[type=checkbox]").click()');
  await click(bob, 'Коды совпадают');
  for (const peer of [alice, bob]) {
    await until(() => execute(peer, 'return !!document.querySelector("[aria-label=Сообщение]")'), 'chat UI');
  }
  return { offer, sas };
}

try {
  for (const port of [4444, 4445]) {
    const result = await request(`http://127.0.0.1:${port}/session`, {
      capabilities: { alwaysMatch: { 'tauri:options': {
        application: process.env.SSC_BINARY || resolve('src-tauri/target/release/ssc'),
      } } },
    });
    sessions.push({ url: `http://127.0.0.1:${port}/session/${result.sessionId}` });
  }
  for (const peer of sessions) {
    await until(() => visible(peer, 'Начать чат'), 'welcome');
    await ipc(peer, 'set_ice_servers', { servers: [] });
    await ipc(peer, 'set_diagnostics', { enabled: true });
  }
  const { offer, sas } = await connect(sessions);
  const text = 'native-smoke-🦀';
  await input(sessions[0], '[aria-label=Сообщение]', text);
  await execute(sessions[0], 'document.querySelector("[aria-label=Отправить]").click()');
  await until(() => visible(sessions[1], text), 'message from UI');
  const reverse = 'native reverse 🦀';
  await input(sessions[1], '[aria-label=Сообщение]', reverse);
  await execute(sessions[1], 'document.querySelector("[aria-label=Отправить]").click()');
  await until(() => visible(sessions[0], reverse), 'reverse message');
  for(const peer of sessions) await until(async()=> (await ipc(peer,'get_session_state')).route==='direct', 'selected direct route');
  console.log('PASS native invitation screens, one-sided wait, SAS confirmation and bidirectional chat');
  for (const peer of sessions) {
    const report = await ipc(peer, 'get_diagnostics');
    const json = JSON.stringify(report);
    if ([text, reverse, offer, sas].some(secret => json.includes(secret))) throw new Error('Sensitive diagnostic output');
    if (!report.enabled || report.events.length === 0) throw new Error('No diagnostic events');
  }
  console.log('PASS native diagnostic redaction');
  await click(sessions[0], 'Диагностика');
  await execute(sessions[0], `const original=URL.createObjectURL;URL.createObjectURL=blob=>{blob.text().then(text=>window.smokeExport=text);return original(blob);};
    const originalClick=HTMLAnchorElement.prototype.click;HTMLAnchorElement.prototype.click=function(){if(this.download)this.download='ssc-smoke-report.json';return originalClick.call(this);};`);
  await click(sessions[0], 'Скачать JSON');
  await until(() => execute(sessions[0], 'return !!window.smokeExport'), 'JSON export');
  const exported=await execute(sessions[0], 'return JSON.parse(window.smokeExport)');
  if (!exported.enabled || !exported.events.length) throw new Error('Empty exported report');
  console.log('PASS diagnostic JSON export in native WebView');
  await ipc(sessions[0], 'disconnect');
  for (const peer of sessions) await until(async () => !await visible(peer, text), 'history cleanup');
  console.log('PASS native final disconnect clears history');
} finally {
  for (const peer of sessions) await request(peer.url, undefined, 'DELETE');
}
