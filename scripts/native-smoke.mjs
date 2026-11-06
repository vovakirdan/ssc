// Run against two tauri-driver instances (4444/4445); no mocked IPC or transport.
import { resolve } from 'node:path';

const sessions = [];
async function request(url, body, method = 'POST') {
  const response = await fetch(url, {
    method, headers: { 'content-type': 'application/json' },
    ...(body ? { body: JSON.stringify(body) } : {}),
  }).then(result => result.json());
  if (response.value?.error) throw new Error(`${response.value.error}: ${response.value.message}`);
  return response.value;
}
function execute(peer, script, args = []) {
  return request(`${peer.url}/execute/sync`, { script, args });
}
async function ipc(peer, command, args = {}) {
  const response = await request(`${peer.url}/execute/async`, {
    script: `const done=arguments[arguments.length-1];
      window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]).then(
        value=>done({ok:true,value}),error=>done({ok:false,error}));`,
    args: [command, args],
  });
  if (!response.ok) throw new Error(JSON.stringify(response.error));
  return response.value;
}
async function until(predicate, description) {
  for (let attempt = 0; attempt < 200; attempt++) {
    if (await predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out: ${description}`);
}
function click(peer, text) {
  return execute(peer, `const button=Array.from(document.querySelectorAll('button')).find(
    button=>button.textContent.includes(arguments[0]));
    if(!button||button.disabled)throw Error('Missing or disabled button');button.click();`, [text]);
}
function input(peer, selector, value) {
  return execute(peer, `const element=document.querySelector(arguments[0]);
    Object.getOwnPropertyDescriptor(Object.getPrototypeOf(element),'value').set.call(element,arguments[1]);
    element.dispatchEvent(new Event('input',{bubbles:true}));`, [selector, value]);
}
async function visible(peer, text) {
  return execute(peer, 'return document.body.innerText.includes(arguments[0])', [text]);
}
async function connect([alice, bob]) {
  for (const peer of [alice, bob]) await click(peer, 'Начать чат');
  await click(alice, 'Создать QR-код');
  await until(() => execute(alice, 'return !!Array.from(document.querySelectorAll("input[readonly],p.font-mono")).find(e=>(e.value||e.textContent).startsWith("v2:"))'), 'offer screen');
  const offer = await execute(alice, 'return Array.from(document.querySelectorAll("input[readonly],p.font-mono")).find(e=>(e.value||e.textContent).startsWith("v2:")).value || Array.from(document.querySelectorAll("p.font-mono")).find(e=>e.textContent.startsWith("v2:")).textContent');
  await click(bob, 'Сканировать QR-код');
  await input(bob, 'textarea', offer);
  await click(bob, 'Принять приглашение');
  await until(() => execute(bob, 'return !!Array.from(document.querySelectorAll("input[readonly],p.font-mono")).find(e=>(e.value||e.textContent).startsWith("v2:"))'), 'answer screen');
  const answer = await execute(bob, 'return Array.from(document.querySelectorAll("input[readonly],p.font-mono")).find(e=>(e.value||e.textContent).startsWith("v2:")).value || Array.from(document.querySelectorAll("p.font-mono")).find(e=>e.textContent.startsWith("v2:")).textContent');
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
        application: process.env.SSC_BINARY || resolve('src-tauri/target/debug/ssc'),
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
