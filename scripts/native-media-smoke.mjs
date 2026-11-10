// Actual native WebView ↔ Chromium UI, local owned helper/TURN, synthetic files only.
// Start an isolated tauri-driver with SSC_FORCE_RELAY=1 and private XDG_DOWNLOAD_DIR.
// SSC_MEDIA_RUNTIME must name the owned fixture; SSC_MEDIA_LARGE=1 also transfers100MiB.
import { chromium } from 'playwright';
import { createHash, randomBytes } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { lstat, realpath, mkdir, mkdtemp, open, rm } from 'node:fs/promises';
import { basename, join, resolve, sep } from 'node:path';
import { request, execute, ipc, input } from './webdriver.mjs';

const origin = process.env.SSC_MEDIA_ORIGIN || 'http://127.0.0.1:1430';
const parsed = new URL(origin);
if (!['127.0.0.1', 'localhost', '[::1]'].includes(parsed.hostname) || parsed.pathname !== '/' || parsed.search || parsed.hash || parsed.username || parsed.password) throw new Error('Owned loopback fixture required');
const driver = process.env.SSC_DRIVER_ORIGIN || 'http://127.0.0.1:4444';
if (!/^http:\/\/127\.0\.0\.1:\d{1,5}$/.test(driver)) throw new Error('Owned loopback driver required');
const runtime = await realpath(process.env.SSC_MEDIA_RUNTIME || '');
const runtimeStat = await lstat(runtime);
if (!runtime.startsWith('/tmp/ssc-') || !runtimeStat.isDirectory() || runtimeStat.uid !== process.getuid() || (runtimeStat.mode & 0o077)) throw new Error('Private fixture directory required');
const downloadRoot = await realpath(join(runtime, 'downloads'));
if (!downloadRoot.startsWith(runtime + sep)) throw new Error('Private fixture downloads required');
const work = await mkdtemp(join(runtime, 'media-'));
await mkdir(join(work, 'browser-downloads'), { mode: 0o700 });
const large = process.env.SSC_MEDIA_LARGE === '1';
const roles = process.env.SSC_MEDIA_ROLES === 'creator' ? ['creator'] : process.env.SSC_MEDIA_ROLES === 'guest' ? ['guest'] : ['creator', 'guest'];
let stage = 'startup';
let activeFilename = null;
const ownedDownloads = new Set();
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
function check(value, code) { if (!value) throw new Error(code); }
async function until(predicate, timeout = 30_000) {
  const deadline = Date.now() + timeout;
  do { if (await predicate()) return; await pause(100); } while (Date.now() < deadline);
  throw new Error('deadline');
}
function nrun(native, fn, args = []) { return execute(native, `return (${fn.toString()})(...arguments)`, args); }
const exactButton = text => { const button = [...document.querySelectorAll('button')].find(button => button.textContent.trim() === text); if (!button || button.disabled) throw new Error('button'); button.click(); };
const attachment = name => {
  const li = [...document.querySelectorAll('.ssc-message')].find(li => li.querySelector('.ssc-file-heading strong')?.textContent === name);
  return li ? { exists: true, accept: [...li.querySelectorAll('button')].some(button => button.textContent.trim() === 'Принять файл'), save: [...li.querySelectorAll('button')].some(button => button.textContent.trim() === 'Сохранить файл'), complete: [...li.querySelectorAll('.ssc-meta')].some(p => p.textContent === 'Передача завершена'), forgotten: [...li.querySelectorAll('.ssc-meta')].some(p => p.textContent === 'Удалено из памяти'), failed: [...li.querySelectorAll('.ssc-meta')].some(p => p.textContent === 'Ошибка передачи'), cancelled: [...li.querySelectorAll('.ssc-meta')].some(p => p.textContent === 'Отменено'), pending: !!li.querySelector('progress'), percent: li.querySelector('progress')?.value ?? null, preview: !!li.querySelector('img,audio,video') } : { exists: false };
};
const attachmentButton = ({ name, text }) => {
  const li = [...document.querySelectorAll('.ssc-message')].find(li => li.querySelector('.ssc-file-heading strong')?.textContent === name);
  const button = [...(li?.querySelectorAll('button') || [])].find(button => button.textContent.trim() === text || button.getAttribute('aria-label') === text);
  if (!button || button.disabled) throw new Error('attachment_button'); button.click();
};
const hasText = text => [...document.querySelectorAll('.ssc-message-text')].some(element => element.textContent === text);
const react = text => {
  const li = [...document.querySelectorAll('.ssc-message')].find(li => li.querySelector('.ssc-message-text')?.textContent === text);
  const button = li?.querySelector('[aria-label="Реакция 🔥"]'); if (!button || button.disabled) throw new Error('reaction_button'); button.click();
};
const hasReaction = text => [...document.querySelectorAll('.ssc-message')].some(li => li.querySelector('.ssc-message-text')?.textContent === text && li.querySelector('[aria-label="Реакция 🔥"] small')?.textContent === '1');
async function digest(path) { const hash = createHash('sha256'); for await (const chunk of createReadStream(path)) hash.update(chunk); return hash.digest('hex'); }
async function fixture(name, size) {
  const path = join(work, name), file = await open(path, 'wx', 0o600), hash = createHash('sha256');
  try { const block = Buffer.alloc(256 * 1024, 0xa5); for (let offset = 0; offset < size; offset += block.length) { const part = block.subarray(0, Math.min(block.length, size - offset)); await file.write(part); hash.update(part); } }
  finally { await file.close(); }
  return { name, path, size, sha256: hash.digest('hex') };
}
async function relay(page) {
  return page.evaluate(async () => {
    const pc = window.__ssc_media_pcs?.at(-1); if (!pc) return false;
    const stats = await pc.getStats(); const transport = [...stats.values()].find(value => value.type === 'transport' && value.selectedCandidatePairId);
    const pair = transport ? stats.get(transport.selectedCandidatePairId) : [...stats.values()].find(value => value.type === 'candidate-pair' && value.nominated && value.state === 'succeeded');
    return !!pair && stats.get(pair.localCandidateId)?.candidateType === 'relay' && stats.get(pair.remoteCandidateId)?.candidateType === 'relay';
  });
}
async function sendNative(native, text) { await input(native, '[aria-label="Сообщение"]', text); await nrun(native, () => document.querySelector('[aria-label="Отправить"]').click()); }
async function sendBrowser(page, text) { await page.getByLabel('Сообщение', { exact: true }).fill(text); await page.getByRole('button', { name: 'Отправить', exact: true }).click(); }
async function exerciseDuringTransfer(native, page, name, sender, requirePending) {
  const suffix = randomBytes(4).toString('hex');
  const textA = `native media control 🦀 ${suffix}`, textB = `browser media control 🔒 ${suffix}`;
  const pending = sender === 'native' ? await page.evaluate(attachment, name) : await nrun(native, attachment, [name]);
  if (requirePending) check(pending.pending && !pending.complete, 'transfer_completed_before_interaction');
  await sendNative(native, textA); await until(() => page.evaluate(hasText, textA));
  await page.evaluate(react, textA); await until(() => nrun(native, hasReaction, [textA]));
  await sendBrowser(page, textB); await until(() => nrun(native, hasText, [textB]));
  await nrun(native, react, [textB]); await until(() => page.evaluate(hasReaction, textB));
  if (requirePending) {
    const after = sender === 'native' ? await page.evaluate(attachment, name) : await nrun(native, attachment, [name]);
    check(after.pending && !after.complete, 'transfer_completed_during_interaction');
  }
}
async function transfer(native, page, sender, size) {
  const name = `${sender}-оригинал ${randomBytes(6).toString('hex')}.sscbin`;
  const file = await fixture(name, size), nativeSender = sender === 'native';
  activeFilename = name;
  stage = nativeSender ? 'native_file_picker' : 'browser_file_picker';
  if (nativeSender) {
    const element = await request(`${native.url}/element`, { using: 'css selector', value: 'input[type="file"]' });
    const id = element['element-6066-11e4-a52e-4f735466cecf']; check(id, 'file_input_missing');
    await request(`${native.url}/element/${id}/value`, { text: file.path });
    await nrun(native, exactButton, ['Отправить файлы']);
  } else {
    await page.getByLabel('Выбрать вложения', { exact: true }).setInputFiles(file.path);
    await page.getByRole('button', { name: 'Отправить файлы', exact: true }).click();
  }
  stage = nativeSender ? 'browser_manual_accept' : 'native_manual_accept';
  const info = () => nativeSender ? page.evaluate(attachment, name) : nrun(native, attachment, [name]);
  await until(async () => (await info()).accept, 120_000);
  check(!(await info()).complete, 'file_transferred_without_consent');
  if (nativeSender) await page.evaluate(attachmentButton, { name, text: 'Принять файл' });
  else await nrun(native, attachmentButton, [{ name, text: 'Принять файл' }]);
  stage = 'text_reactions_during_transfer';
  await exerciseDuringTransfer(native, page, name, sender, size >= 100 * 1024 * 1024);
  stage = nativeSender ? 'browser_file_receive' : 'native_file_receive';
  await until(async () => (await info()).complete, 600_000);
  check(!(await info()).preview, 'generic_file_embedded_preview');
  stage = nativeSender ? 'browser_original_save' : 'native_original_save';
  let saved;
  if (nativeSender) {
    const downloading = page.waitForEvent('download', { timeout: 30_000 });
    await page.evaluate(attachmentButton, { name, text: 'Сохранить файл' });
    const download = await downloading; check(download.suggestedFilename() === name, 'browser_filename_changed');
    saved = join(work, 'browser-downloads', name); await download.saveAs(saved);
  } else {
    // Verify the engine's directory resolver before every Save. Never write user Downloads.
    const actual = await ipc(native, 'plugin:path|resolve_directory', { directory: 7 });
    check(await realpath(actual) === downloadRoot, 'native_download_path_outside_fixture');
    saved = join(downloadRoot, name); ownedDownloads.add(saved);
    await nrun(native, attachmentButton, [{ name, text: 'Сохранить файл' }]);
    await until(async () => { try { const st = await lstat(saved); return st.isFile() && !st.isSymbolicLink() && st.size === size; } catch { return false; } }, 120_000);
  }
  check((await lstat(saved)).size === file.size && await digest(saved) === file.sha256, 'original_bytes_changed');
  stage = 'attachment_memory_cleanup';
  await until(async () => (await page.evaluate(attachment, name)).complete && (await nrun(native, attachment, [name])).complete);
  await page.evaluate(attachmentButton, { name, text: 'Удалить сообщение из памяти' });
  await nrun(native, attachmentButton, [{ name, text: 'Удалить сообщение из памяти' }]);
  await until(async () => {
    const a = await page.evaluate(attachment, name), b = await nrun(native, attachment, [name]);
    return a.forgotten && !a.save && !a.preview && b.forgotten && !b.save && !b.preview;
  });
  await rm(saved); ownedDownloads.delete(saved); await rm(file.path);
  activeFilename = null;
  console.log(`PASS ${sender}_to_${nativeSender ? 'browser' : 'native'} ${size / 1024 / 1024}MiB: manual consent, Unicode filename, generic Blob Save, exact SHA256, text/reactions and RAM removal`);
}

async function run(role) {
  let native, browser, context, page; const rooms = new Map(), pendingResponses = new Set();
  try {
    stage = 'native_startup';
    const result = await request(`${driver}/session`, { capabilities: { alwaysMatch: { 'tauri:options': { application: process.env.SSC_BINARY || resolve('src-tauri/target/release/ssc') } } } });
    native = { url: `${driver}/session/${result.sessionId}` };
    await request(`${native.url}/timeouts`, { script: 30_000, pageLoad: 60_000, implicit: 0 });
    await until(() => nrun(native, () => [...document.querySelectorAll('button')].some(button => button.textContent.includes('Начать чат') && !button.disabled)));
    await ipc(native, 'set_diagnostics', { enabled: true });
    await ipc(native, 'clear_diagnostics');
    stage = 'native_owned_download_directory';
    const downloadDirectory = await ipc(native, 'plugin:path|resolve_directory', { directory: 7 });
    check(await realpath(downloadDirectory) === downloadRoot, 'native_download_path_outside_fixture');
    const media = await nrun(native, () => ({ secure: isSecureContext, devices: !!navigator.mediaDevices, capture: typeof navigator.mediaDevices?.getUserMedia === 'function' }));
    check(media.secure && media.devices && media.capture, 'native_media_api_unavailable');
    console.log('PASS native secure context/mediaDevices availability and isolated Downloads resolver; no microphone activated');
    browser = await chromium.launch({ headless: true });
    context = await browser.newContext({ acceptDownloads: true }); context.setDefaultTimeout(30_000);
    await context.addInitScript(() => { const Original = window.RTCPeerConnection; window.__ssc_media_pcs = []; window.RTCPeerConnection = class extends Original { constructor(config) { super({ ...config, iceTransportPolicy: 'relay' }); window.__ssc_media_pcs.push(this); } }; });
    page = await context.newPage();
    page.on('response', response => {
      const url = new URL(response.url());
      if (url.origin !== parsed.origin || response.request().method() !== 'POST' || !/^\/v1\/rooms(?:\/[A-Za-z0-9_-]{43}\/join)?$/.test(url.pathname) || !response.ok()) return;
      const task = response.json().then(data => { const token = data.creator_token || data.guest_token; if (/^[A-Za-z0-9_-]{43}$/.test(data.id) && /^[A-Za-z0-9_-]{43}$/.test(token)) rooms.set(data.id, token); }).catch(() => undefined);
      pendingResponses.add(task); void task.finally(() => pendingResponses.delete(task));
    });
    stage = 'room_invitation';
    await nrun(native, exactButton, ['Начать чат']);
    if (role === 'creator') {
      await nrun(native, () => document.querySelector('[aria-label="Временный чат без установки"]').click());
      await nrun(native, exactButton, ['Создать ссылку']);
      let link;
      await until(async () => { link = await nrun(native, () => document.querySelector('[aria-label="Короткая ссылка"]')?.value); return !!link; }, 60_000);
      check(new URL(link).origin === parsed.origin, 'unexpected_invitation_origin');
      await page.goto(link); await page.getByRole('button', { name: 'Войти', exact: true }).click();
    } else {
      await page.goto(origin); await page.getByRole('button', { name: 'Создать чат', exact: true }).click();
      await page.getByLabel('Ссылка приглашения', { exact: true }).waitFor();
      const link = await page.getByLabel('Ссылка приглашения', { exact: true }).inputValue();
      await nrun(native, () => document.querySelector('[aria-label="Подключиться по короткой ссылке"]').click());
      await input(native, '[aria-label="Ссылка приглашения"]', link);
      await nrun(native, exactButton, ['Подключиться по ссылке']);
    }
    stage = 'mandatory_sas';
    await until(() => nrun(native, () => !!document.querySelector('input[type="checkbox"]')), 60_000);
    await page.getByLabel('Код сверки').waitFor();
    const nativeSas = await ipc(native, 'get_fingerprint');
    check(nativeSas?.sas && nativeSas.sas === await page.getByLabel('Код сверки').textContent(), 'sas_mismatch');
    check(!await ipc(native, 'get_sas_status'), 'automatic_sas_confirmation');
    check(await page.getByRole('button', { name: 'Коды совпадают', exact: true }).isDisabled(), 'browser_sas_gate_missing');
    check(await nrun(native, () => [...document.querySelectorAll('button')].find(button => button.textContent.includes('Коды совпадают')).disabled), 'native_sas_gate_missing');
    await nrun(native, () => document.querySelector('input[type="checkbox"]').click());
    await nrun(native, exactButton, ['Коды совпадают — продолжить']);
    check(!await nrun(native, () => !!document.querySelector('[aria-label="Сообщение"]')), 'one_sided_chat');
    await page.getByRole('checkbox').check(); await page.getByRole('button', { name: 'Коды совпадают', exact: true }).click();
    await until(async () => (await ipc(native, 'get_session_state')).phase === 'connected', 60_000);
    await page.getByLabel('Сообщение', { exact: true }).waitFor();
    await until(async () => (await ipc(native, 'get_session_state')).route === 'relay' && await relay(page), 30_000);
    await Promise.allSettled(pendingResponses);
    check(rooms.size >= 1, 'browser_room_receipt_missing');
    const snapshot = await ipc(native, 'get_session_state');
    let staleRejected = false;
    try { await ipc(native, 'request_voice_capture', { generation: snapshot.generation - 1 }); } catch { staleRejected = true; }
    check(staleRejected, 'stale_microphone_request_accepted');
    console.log(`PASS native_${role}: equal full SAS, both UI confirmation gates and selected relay/relay`);
    for (const size of [1024 * 1024, ...(large ? [100 * 1024 * 1024] : [])]) {
      await transfer(native, page, 'native', size); await transfer(native, page, 'browser', size);
    }
    stage = 'authenticated_exit';
    if (role === 'creator') {
      const wideExit = page.getByRole('button', { name: 'Выйти и завершить чат', exact: true });
      if (await wideExit.isVisible()) await wideExit.click(); else await page.getByRole('button', { name: 'Выйти из чата', exact: true }).click();
      await page.getByRole('button', { name: 'Выйти и очистить', exact: true }).click();
      await until(async () => (await ipc(native, 'get_session_state')).close_reason === 'peer_left');
      check(await nrun(native, () => document.querySelectorAll('.ssc-message').length === 0 && document.body.innerText.includes('Собеседник вышел')), 'native_peer_exit_history');
    } else {
      await nrun(native, () => document.querySelector('[aria-label="Выйти из чата"]').click());
      await nrun(native, exactButton, ['Выйти и очистить']);
      await page.getByText('Собеседник вышел. История чата на этом устройстве очищена.', { exact: true }).waitFor();
      check(await page.locator('.ssc-message').count() === 0, 'browser_peer_exit_history');
      check((await ipc(native, 'get_session_state')).close_reason === 'local_left', 'native_local_exit_reason');
    }
    console.log(`PASS native_${role}: authenticated exit reason and immediate history clearing`);
  } catch (error) {
    console.error(`FAIL native_${role} stage=${stage}; raw errors and private contents withheld`);
    if (native) try {
      const s = await ipc(native, 'get_session_state');
      console.error(JSON.stringify({ phase: ['idle','invitation','ice','key_exchange','verification','waiting_peer','connected','recovering','closed'].includes(s.phase) ? s.phase : 'unknown', relay: s.route === 'relay', closeReason: ['local_left','peer_left','expired','connection_lost'].includes(s.close_reason) ? s.close_reason : null, attachment: activeFilename ? await nrun(native, attachment, [activeFilename]) : null }));
    } catch { /* Never print IPC errors. */ }
    if (page) try {
      console.error(JSON.stringify(await page.evaluate(name => ({ connection: window.__ssc_media_pcs?.at(-1)?.connectionState ?? null, ice: window.__ssc_media_pcs?.at(-1)?.iceConnectionState ?? null, completed: [...document.querySelectorAll('.ssc-attachment .ssc-meta')].filter(p => p.textContent === 'Передача завершена').length, transferError: document.body.innerText.includes('Передача прервана'), integrityError: document.body.innerText.includes('Файл не прошёл проверку целостности') }), activeFilename)));
    } catch { /* Never print browser errors or DOM text. */ }
    throw new Error('media_acceptance_failed');
  } finally {
    await Promise.allSettled(pendingResponses);
    // Browser pagehide aborts its fetches; delete owned rooms in Node before closing it.
    let cleaned = true;
    for (const [id, token] of rooms) {
      let deleted = false;
      for (let attempt = 0; attempt < 3 && !deleted; attempt++) {
        try {
          const response = await fetch(`${parsed.origin}/v1/rooms/${id}`, { method: 'DELETE', headers: { authorization: `Bearer ${token}` }, signal: AbortSignal.timeout(5000) });
          deleted = [200, 204, 404, 410].includes(response.status); await response.body?.cancel();
        } catch { /* Bounded retry; no raw HTTP error or capability output. */ }
        if (!deleted) await pause(150 * (attempt + 1));
      }
      cleaned &&= deleted;
    }
    if (!cleaned) { console.error('FAIL owned room cleanup'); process.exitCode = 1; }
    rooms.clear();
    if (native) { try { await ipc(native, 'disconnect', { reason: 'error' }); } catch { /* Already closed. */ } try { await request(native.url, undefined, 'DELETE'); } catch { /* Driver already stopped. */ } }
    if (browser) await browser.close();
  }
}
try { for (const role of roles) await run(role); console.log('PASS native media acceptance completed'); }
catch { process.exitCode = 1; }
finally { for (const path of ownedDownloads) { if (path.startsWith(downloadRoot + sep) && basename(path).endsWith('.sscbin')) await rm(path, { force: true }); } await rm(work, { recursive: true, force: true }); }
