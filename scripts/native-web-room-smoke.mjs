// Real Chromium + native WebView, real room service, no mocked transport or crypto.
import { chromium } from 'playwright';
import { resolve } from 'node:path';
import { request, ipc, execute, until, visible, click, input } from './webdriver.mjs';
let stage = 'startup', native, browser, page;
try {
  browser = await chromium.launch({ args: ['--allow-loopback-in-peer-connection', '--disable-features=WebRtcHideLocalIpsWithMdns'] });
  const context = await browser.newContext();
  await context.addInitScript(forceRelay => {
    const Original = window.RTCPeerConnection;
    window.__ssc_test_pc = [];
    window.__ssc_test_channels = [];
    window.__ssc_test_history = [];
    window.RTCPeerConnection = class extends Original {
      constructor(config) { super(forceRelay ? { ...config, iceTransportPolicy: 'relay' } : config); window.__ssc_test_pc.push(this); this.addEventListener('connectionstatechange', () => { const local = this.localDescription?.sdp || '', remote = this.remoteDescription?.sdp || ''; window.__ssc_test_history.push({ connection: this.connectionState, ice: this.iceConnectionState, localRelay: (local.match(/ typ relay/g) || []).length, remoteRelay: (remote.match(/ typ relay/g) || []).length, localCandidates: (local.match(/a=candidate:/g) || []).length, remoteCandidates: (remote.match(/a=candidate:/g) || []).length }); }); this.addEventListener('datachannel', event => { const c = event.channel; window.__ssc_test_channels.push({ expectedLabel: c.label === 'ssc-v3', ordered: c.ordered, retries: c.maxRetransmits, lifetime: c.maxPacketLifeTime }); }); }
    };
  }, process.env.SSC_TEST_BROWSER_RELAY === '1');
  page = await context.newPage();
  const result = await request('http://127.0.0.1:4444/session', { capabilities: { alwaysMatch: { 'tauri:options': { application: process.env.SSC_BINARY || resolve('src-tauri/target/release/ssc') } } } });
  native = { url: `http://127.0.0.1:4444/session/${result.sessionId}` };
  await until(() => visible(native, 'Начать чат'), 'native welcome');
  await ipc(native, 'set_diagnostics', { enabled: true });
  stage = 'native_room';
  await ipc(native, 'create_temporary_room');
  await until(async () => !!(await ipc(native, 'get_link_state'))?.url, 'native room link');
  const link = (await ipc(native, 'get_link_state')).url;
  stage = 'browser_join';
  await page.goto(link);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  stage = 'verification';
  await until(() => visible(native, 'Я вижу одинаковый код'), 'native verification');
  const sas = await ipc(native, 'get_fingerprint');
  await page.getByLabel('Код сверки').waitFor();
  if (await page.getByLabel('Код сверки').textContent() !== sas.sas) throw new Error('Verification mismatch');
  await execute(native, 'document.querySelector("input[type=checkbox]").click()');
  await click(native, 'Коды совпадают');
  await page.getByRole('checkbox', { name: 'Я сверил(а) весь код с собеседником' }).check();
  await page.getByRole('button', { name: 'Коды совпадают', exact: true }).click();
  stage = 'chat';
  await until(async () => (await ipc(native, 'get_session_state')).phase === 'connected', 'native connected');
  if (process.env.SSC_TEST_DIRECT !== '1') await until(async () => (await ipc(native, 'get_session_state')).route === 'relay', 'native relay');
  await page.getByLabel('Сообщение').fill('browser to native 🦀');
  await page.getByRole('button', { name: 'Отправить', exact: true }).click();
  await until(() => visible(native, 'browser to native 🦀'), 'native receives browser');
  await input(native, '[aria-label="Сообщение"]', 'native to browser 🦀');
  await execute(native, 'document.querySelector(\'[aria-label="Отправить"]\').click()');
  await page.getByText('native to browser 🦀', { exact: true }).waitFor();
  stage = 'closure';
  await page.getByRole('button', { name: 'Выйти и завершить чат' }).click();
  await until(async () => (await ipc(native, 'get_session_state')).phase === 'closed', 'native closes with browser');
  console.log(`PASS native creator ↔ Chromium guest: shared v3, equal SAS, mandatory confirmation, ${process.env.SSC_TEST_DIRECT === '1' ? 'direct control' : 'forced relay'}, bidirectional Unicode and close`);
} catch {
  console.error(`FAIL native-browser stage=${stage}; sensitive data withheld`);
  if (native) try {
    const state = await ipc(native, 'get_session_state');
    const report = await ipc(native, 'get_diagnostics');
    console.error(JSON.stringify({ phase: state.phase, route: state.route, events: report.events.slice(-8).map(event => ({ stage: event.stage, outcome: event.outcome, code: event.code })) }));
  } catch { /* No raw failures. */ }
  if (page) try { console.error(JSON.stringify(await page.evaluate(() => ({ history: window.__ssc_test_history, transport: window.__ssc_test_pc.map(pc => ({ connection: pc.connectionState, ice: pc.iceConnectionState })), channels: window.__ssc_test_channels, errors: ['Недопустимый канал','Ошибка защищённого канала','Не удалось безопасно','Соединение завершено','Обмен ключами','Чат закрыт'].filter(text => document.body.innerText.includes(text)), verificationVisible: !!document.querySelector('[aria-label="Код сверки"]'), ended: document.body.innerText.includes('Чат завершён') })))); } catch { /* No raw failures. */ }
  process.exitCode = 1;
} finally {
  if (native) { try { await ipc(native, 'disconnect'); await request(native.url, undefined, 'DELETE'); } catch { /* Closed app. */ } }
  if (browser) await browser.close();
}
