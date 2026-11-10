// Two native release windows against the live HTTPS helper and authenticated TURN.
// Drivers must use isolated SSC_PROFILE_DIR, SSC_FORCE_RELAY=1 and the same transport.
import { resolve } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { readFileSync, writeFileSync } from 'node:fs';
import { request, execute, ipc, until, click, input, visible } from './webdriver.mjs';
const run = promisify(execFile);
const sessions = [];
const secrets = [];
let stage = 'startup';
let helperStopped = false;
let revocationObserved = false;
const transport = process.env.SSC_TURN_TRANSPORT || 'udp';
const activationProfiles = process.env.SSC_ACTIVATION_PROFILES ? JSON.parse(process.env.SSC_ACTIVATION_PROFILES) : null;
const accessReferences = [];
if (activationProfiles && (!Array.isArray(activationProfiles) || activationProfiles.length !== 2 || !activationProfiles.every(path => typeof path === 'string'))) throw new Error('Invalid activation test inputs');
if (!['udp', 'tcp', 'tls'].includes(transport)) throw new Error('Unsupported test transport');
function byLabel(peer, label) { return execute(peer, 'return document.querySelector(arguments[0])?.value ?? null', [`[aria-label="${label}"]`]); }
function hasLabel(peer, label) { return execute(peer, 'return !!document.querySelector(arguments[0])', [`[aria-label="${label}"]`]); }
function clickLabel(peer, label) { return execute(peer, 'document.querySelector(arguments[0]).click()', [`[aria-label="${label}"]`]); }
async function control(command) {
  if (!['stop-helper', 'start-helper'].includes(command)) throw new Error('Invalid helper control');
  await run('ssh', ['-i', '/home/zov/.local/share/ssc-deploy/ci_ed25519', '-o', 'IdentitiesOnly=yes', '-o', 'UserKnownHostsFile=/home/zov/.local/share/ssc-deploy/known_hosts', '-o', 'StrictHostKeyChecking=yes', '-o', 'BatchMode=yes', 'ssc-deploy@104.194.156.226', command], { timeout: 30000, maxBuffer: 4096 });
}
async function send(sender, receiver, text) {
  secrets.push(text);
  await input(sender, '[aria-label="Сообщение"]', text);
  await clickLabel(sender, 'Отправить');
  await until(() => visible(receiver, text), 'received chat message');
}
try {
  for (const port of [4444, 4445]) {
    const result = await request(`http://127.0.0.1:${port}/session`, { capabilities: { alwaysMatch: { 'tauri:options': { application: process.env.SSC_BINARY || resolve('src-tauri/target/release/ssc') } } } });
    sessions.push({ url: `http://127.0.0.1:${port}/session/${result.sessionId}` });
  }
  stage = 'automatic_activation';
  for (const peer of sessions) {
    await until(() => visible(peer, 'Начать чат'), 'welcome UI');
    if (activationProfiles) {
      stage = 'code_activation';
      if ((await ipc(peer, 'get_access_status')).state !== 'inactive') throw new Error('Code activation requires a fresh generic profile');
      const token = JSON.parse(readFileSync(activationProfiles[sessions.indexOf(peer)], 'utf8')).bootstrap_token;
      if (typeof token !== 'string' || !/^[A-Za-z0-9_-]{43}$/.test(token)) throw new Error('Invalid activation test code');
      secrets.push(token);
      await ipc(peer, 'set_diagnostics', { enabled: true });
      await click(peer, 'Настройки');
      await until(() => hasLabel(peer, 'Код активации'), 'activation code form');
      await until(() => execute(peer, 'return document.querySelector(\'[aria-label="Код активации"]\')?.disabled === false'), 'activation code form ready');
      await input(peer, '[aria-label="Код активации"]', token);
      await click(peer, 'Активировать');
      await until(async () => (await ipc(peer, 'get_access_status')).state === 'active', 'code activation');
      const access = await ipc(peer, 'get_access_status');
      accessReferences.push(access.access_reference);
      if (process.env.SSC_ACTIVATION_REFERENCES_FILE) writeFileSync(process.env.SSC_ACTIVATION_REFERENCES_FILE, JSON.stringify(accessReferences), { mode: 0o600 });
      await until(async () => !(await byLabel(peer, 'Код активации')), 'activation input cleared');
      const saved = await execute(peer, 'return JSON.stringify(localStorage)');
      if (saved.includes(token)) throw new Error('Activation code persisted in frontend storage');
      await click(peer, 'Сохранить настройки');
      await until(() => visible(peer, 'Начать чат'), 'return from activation');
    }
    const access = await ipc(peer, 'initialize_access');
    if (access.state !== 'active') throw new Error(JSON.stringify(access.error || { code: 'activation_not_active' }));
    // Do not fetch get_ice_servers: it can contain credentials in custom mode.
    await ipc(peer, 'set_diagnostics', { enabled: true });
    await click(peer, 'Начать чат');
    await clickLabel(peer, 'Подключиться по короткой ссылке');
    await until(() => hasLabel(peer, 'Ссылка приглашения'), 'link form');
  }
  console.log(`PASS ${transport} ${activationProfiles ? 'UI code' : 'automatic'} activation of two isolated native clients`);
  stage = 'create_live_link';
  await click(sessions[0], 'Создать ссылку');
  await until(() => byLabel(sessions[0], 'Короткая ссылка'), 'live invitation');
  const invitation = await byLabel(sessions[0], 'Короткая ссылка');
  const parsed = new URL(invitation);
  if (parsed.origin !== 'https://connect.vovakirdan.ru' || !/^\/i\/[A-Za-z0-9_-]{43}$/.test(parsed.pathname)) throw new Error('Invalid live invitation');
  secrets.push(invitation, parsed.pathname.split('/').at(-1));
  stage = 'guest_auto_answer';
  await input(sessions[1], '[aria-label="Ссылка приглашения"]', invitation);
  await click(sessions[1], 'Подключиться по ссылке');
  stage = 'verification';
  for (const peer of sessions) await until(() => visible(peer, 'Я вижу одинаковый код'), 'verification UI');
  const a = await ipc(sessions[0], 'get_fingerprint');
  const b = await ipc(sessions[1], 'get_fingerprint');
  if (!a?.sas || a.sas !== b?.sas) throw new Error('SAS mismatch');
  secrets.push(a.sas);
  if (await ipc(sessions[0], 'get_sas_status') || await ipc(sessions[1], 'get_sas_status')) throw new Error('SAS was confirmed without user action');
  await execute(sessions[0], 'document.querySelector("input[type=checkbox]").click()');
  await click(sessions[0], 'Коды совпадают');
  await until(() => visible(sessions[0], 'Ожидаем подтверждения собеседника'), 'one-sided confirmation');
  await execute(sessions[1], 'document.querySelector("input[type=checkbox]").click()');
  await click(sessions[1], 'Коды совпадают');
  for (const peer of sessions) await until(() => hasLabel(peer, 'Сообщение'), 'chat');
  stage = 'relay_route';
  for (const peer of sessions) await until(async () => (await ipc(peer, 'get_session_state')).route === 'relay', 'selected relay route');
  console.log(`PASS ${transport} live HTTPS invitation, automatic answer, mandatory two-sided SAS and relay route`);
  stage = 'capture_status';
  const platform = { win32: 'windows', linux: 'linux', darwin: 'macos' }[process.platform] || 'other';
  let captureRequested = 0;
  for (const peer of sessions) {
    const capture = await ipc(peer, 'get_capture_protection');
    if (capture?.platform !== platform || typeof capture.enabled !== 'boolean' || typeof capture.limitation !== 'string' || !capture.limitation.trim()) throw new Error('Invalid capture protection status');
    if (platform !== 'windows' && capture.enabled) throw new Error('Unsupported capture protection claim');
    if (capture.enabled) captureRequested++;
    await clickLabel(peer, 'Как защищён чат');
    const description = capture.enabled ? 'Системная защита окна от части способов захвата включена.' : 'Системная защита от захвата экрана здесь не подтверждена.';
    await until(async () => await visible(peer, description) && await visible(peer, capture.limitation), 'honest capture status in help');
    await clickLabel(peer, 'Закрыть окно');
  }
  console.log(`PASS ${transport} honest capture status and help: API requested=${captureRequested}/2; physical capture remains untested`);
  stage = 'bidirectional_messages';
  await send(sessions[0], sessions[1], `managed ${transport} greeting 🦀`);
  await send(sessions[1], sessions[0], `managed ${transport} reply 🦀`);
  console.log(`PASS ${transport} bidirectional messages through server TURN`);
  const hold = Number(process.env.SSC_HOLD_SECONDS || 0);
  if (!Number.isInteger(hold) || hold < 0 || hold > 1800) throw new Error('Invalid hold duration');
  if (hold) {
    stage = 'credential_expiry_hold';
    const started = Date.now();
    while (Date.now() - started < hold * 1000) {
      await new Promise(resolve => setTimeout(resolve, Math.min(60000, hold * 1000 - (Date.now() - started))));
      const elapsed = Math.floor((Date.now() - started) / 1000);
      await send(sessions[0], sessions[1], `managed ${transport} hold ${elapsed} greeting`);
      await send(sessions[1], sessions[0], `managed ${transport} hold ${elapsed} reply`);
      console.log(`PASS ${transport} active relay chat at ${elapsed}s without a new peer or credential request`);
    }
    console.log(`PASS ${transport} existing chat survived credential expiry hold`);
  }
  if (process.env.SSC_REVOKE_READY_FILE) {
    stage = 'external_revocation';
    writeFileSync(process.env.SSC_REVOKE_READY_FILE, 'ready\n', { mode: 0o600, flag: 'wx' });
    console.log(`READY ${transport} existing relay chat for external administrative revocation`);
    const deadline = Date.now() + 120000;
    while (Date.now() < deadline) {
      const states = await Promise.all(sessions.map(peer => ipc(peer, 'get_session_state')));
      if (states.every(state => ['closed', 'idle'].includes(state.phase))) { revocationObserved = true; break; }
      await new Promise(resolve => setTimeout(resolve, 500));
    }
    if (!revocationObserved) throw new Error('Revoked sessions did not retire');
    console.log(`PASS ${transport} both native sessions retired after external revocation`);
  }
  if (process.env.SSC_TEST_HELPER_OUTAGE === '1') {
    stage = 'helper_outage';
    await control('stop-helper'); helperStopped = true;
    await send(sessions[0], sessions[1], `managed ${transport} helper-offline greeting 🦀`);
    await send(sessions[1], sessions[0], `managed ${transport} helper-offline reply 🦀`);
    await control('start-helper'); helperStopped = false;
    console.log(`PASS ${transport} established chat survives helper shutdown`);
  }
  stage = 'diagnostic_redaction';
  for (const peer of sessions) {
    const report = JSON.stringify(await ipc(peer, 'get_diagnostics'));
    if (secrets.some(value => report.includes(value))) throw new Error('Sensitive diagnostic output');
    if (!revocationObserved && (await ipc(peer, 'get_link_state'))?.phase !== 'complete') throw new Error('Link not complete');
  }
  console.log(`PASS ${transport} diagnostic redaction${revocationObserved ? '' : ' and helper completion'}`);
  stage = 'cleanup';
  if (!revocationObserved) {
    await clickLabel(sessions[0], 'Выйти из чата');
    await until(() => visible(sessions[0], 'Завершить чат?'), 'exit confirmation');
    await click(sessions[0], 'Остаться');
    for (const peer of sessions) if ((await ipc(peer, 'get_session_state')).phase !== 'connected' || !await hasLabel(peer, 'Сообщение')) throw new Error('Cancelled exit disconnected the chat');
    await clickLabel(sessions[0], 'Выйти из чата');
    await until(() => visible(sessions[0], 'Завершить чат?'), 'exit confirmation again');
    await click(sessions[0], 'Выйти и очистить');
  }
  for (const peer of sessions) await until(async () => ['idle', 'closed'].includes((await ipc(peer, 'get_session_state')).phase), 'session retirement');
  if (!revocationObserved) {
    if ((await ipc(sessions[0], 'get_session_state')).close_reason !== 'local_left' || (await ipc(sessions[1], 'get_session_state')).close_reason !== 'peer_left') throw new Error('Authenticated exit reason mismatch');
    for (const peer of sessions) await until(() => execute(peer, 'return document.querySelectorAll(".ssc-message").length === 0'), 'cleared conversation history');
  }
  console.log(`PASS ${transport} session cleanup`);
} catch (error) {
  let code = 'webdriver_or_timeout';
  try { const parsed = JSON.parse(error.message); if (/^[a-z_]{1,64}$/.test(parsed.code)) code = parsed.code; } catch { /* Do not log untrusted IPC/WebDriver messages. */ }
  console.error(`FAIL ${transport} stage=${stage} code=${code}`);
  for (const peer of sessions) {
    try {
      const state = await ipc(peer, 'get_session_state');
      const link = await ipc(peer, 'get_link_state');
      const diagnostics = await ipc(peer, 'get_diagnostics');
      console.error(JSON.stringify({ phase: state.phase, route: state.route, linkPhase: link.phase, linkError: link.error?.code,
        events: diagnostics.events.slice(-10).map(event => ({ stage: event.stage, outcome: event.outcome, code: event.code })) }));
    } catch { console.error('Diagnostic snapshot unavailable'); }
  }
  process.exitCode = 1;
} finally {
  if (helperStopped) { try { await control('start-helper'); } catch { console.error('FAIL helper restoration'); process.exitCode = 1; } }
  for (const peer of sessions) {
    try { await ipc(peer, 'disconnect'); } catch { /* Application may already have exited. */ }
    try { await request(peer.url, undefined, 'DELETE'); } catch { /* Application may already have exited. */ }
  }
}
