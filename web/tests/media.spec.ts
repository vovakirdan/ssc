import { test, expect, type Browser, type Page, type BrowserContext } from '@playwright/test';
import { createHash } from 'node:crypto';
import { crc32, deflateSync } from 'node:zlib';
import { peerContext, openInvitation, confirm, trackOwnedRoom, leaveChat } from './helpers';
async function conversation(browser: Browser) {
  const contexts: BrowserContext[] = []; let cleanup: () => Promise<void> = async () => {};
  try {
    const a = await peerContext(browser), b = await peerContext(browser); contexts.push(a, b);
    const creator = await a.newPage(), guest = await b.newPage();
    cleanup = trackOwnedRoom(creator);
    await creator.clock.install({ time: new Date() });
    await creator.goto('/'); await creator.getByRole('button', { name: 'Создать чат', exact: true }).click();
    const field = creator.getByLabel('Ссылка приглашения'); await expect(field).toBeVisible();
    await openInvitation(guest, await field.inputValue()); await guest.getByRole('button', { name: 'Войти', exact: true }).click(); await confirm(creator, guest);
    return { creator, guest, close: async () => { await cleanup(); for (const context of contexts) await context.close(); } };
  } catch (error) { await cleanup(); for (const context of contexts) await context.close(); throw error; }
}
async function reveal(page: Page) { const button = page.getByRole('button', { name: 'Показать чат', exact: true }); if (await button.isVisible()) await button.click(); }
async function offer(page: Page, name: string, mimeType: string, buffer: Buffer) {
  await reveal(page); await page.getByLabel('Выбрать вложения', { exact: true }).setInputFiles({ name, mimeType, buffer });
  const dialog = page.getByRole('dialog', { name: 'Отправить вложения?' }); await expect(dialog).toBeVisible();
  await expect(dialog.getByText(/Содержимое и метаданные/)).toBeVisible(); await dialog.getByRole('button', { name: 'Отправить файлы', exact: true }).click();
  await expect(dialog).toHaveCount(0);
}
function photoWithExif() {
  const chunk = (name: string, data: Buffer) => { const type = Buffer.from(name), size = Buffer.alloc(4), checksum = Buffer.alloc(4); size.writeUInt32BE(data.length); checksum.writeUInt32BE(crc32(Buffer.concat([type, data]))); return Buffer.concat([size, type, data, checksum]); };
  const header = Buffer.alloc(13); header.writeUInt32BE(1, 0); header.writeUInt32BE(1, 4); header[8] = 8; header[9] = 2;
  // Actual TIFF DateTime tag in a PNG eXIf chunk. Originals must survive byte-for-byte.
  const exif = Buffer.alloc(46); exif.write('II'); exif.writeUInt16LE(42, 2); exif.writeUInt32LE(8, 4); exif.writeUInt16LE(1, 8); exif.writeUInt16LE(0x132, 10); exif.writeUInt16LE(2, 12); exif.writeUInt32LE(20, 14); exif.writeUInt32LE(26, 18); exif.write('2026:11:10 01:02:03\0', 26);
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]), chunk('IHDR', header), chunk('eXIf', exif), chunk('IDAT', deflateSync(Buffer.from([0,20,180,140]))), chunk('IEND', Buffer.alloc(0))]);
}
async function downloadBytes(page: Page): Promise<Buffer> { const pending = page.waitForEvent('download'); await page.getByRole('button', { name: 'Сохранить файл', exact: true }).click(); const download = await pending; const stream = await download.createReadStream(); if (!stream) throw new Error('owned download unavailable'); const chunks: Buffer[] = []; for await (const chunk of stream) chunks.push(Buffer.from(chunk)); return Buffer.concat(chunks); }

test('encrypted photo retains EXIF and original bytes, with no autoplay or persistent media', async ({ browser }) => {
  const room = await conversation(browser);
  try {
    const bytes = photoWithExif(); await offer(room.creator, 'owned-exif.png', 'image/png', bytes); await reveal(room.guest);
    await expect(room.guest.getByRole('button', { name: 'Сохранить файл', exact: true })).toBeEnabled();
    const actual = await downloadBytes(room.guest); expect(createHash('sha256').update(actual).digest('hex') === createHash('sha256').update(bytes).digest('hex')).toBe(true);
    expect(actual.equals(bytes), 'Entire original and TIFF metadata must remain unchanged').toBe(true);
    expect(await room.guest.evaluate(() => Object.keys(localStorage).every(key => key === 'ssc-theme') && sessionStorage.length === 0)).toBe(true);
  } finally { await room.close(); }
});
test('binary file needs consent and text stays usable during transfer', async ({ browser }) => {
  const room = await conversation(browser);
  try {
    const bytes = Buffer.alloc(2 * 1024 * 1024, 73); await offer(room.creator, 'owned-original.bin', 'application/octet-stream', bytes); await reveal(room.guest);
    await expect(room.guest.getByRole('button', { name: 'Принять файл', exact: true })).toBeVisible();
    await expect(room.guest.getByRole('button', { name: 'Сохранить файл', exact: true })).toHaveCount(0);
    await room.guest.getByRole('button', { name: 'Принять файл', exact: true }).click();
    await room.creator.getByLabel('Сообщение', { exact: true }).fill('Text while file streams 🦀'); await room.creator.getByRole('button', { name: 'Отправить', exact: true }).click();
    await expect(room.guest.getByText('Text while file streams 🦀', { exact: true })).toBeVisible();
    await expect(room.guest.getByRole('button', { name: 'Сохранить файл', exact: true })).toBeEnabled(); expect((await downloadBytes(room.guest)).equals(bytes)).toBe(true);
  } finally { await room.close(); }
});
test('authenticated exit after twenty seconds clears history and keeps the precise reason', async ({ browser }) => {
  const room = await conversation(browser);
  try {
    await room.creator.getByLabel('Сообщение', { exact: true }).fill('Owned ephemeral message'); await room.creator.getByRole('button', { name: 'Отправить', exact: true }).click(); await expect(room.guest.getByText('Owned ephemeral message', { exact: true })).toBeVisible();
    await room.creator.clock.fastForward(25_000);
    await leaveChat(room.guest);
    await expect(room.creator.getByText('Собеседник вышел. История чата на этом устройстве очищена.', { exact: true })).toBeVisible();
    await expect(room.creator.getByText('Owned ephemeral message', { exact: true })).toHaveCount(0); await expect(room.creator.getByLabel('Переписка')).toHaveCount(0);
  } finally { await room.close(); }
});
test('voice uses a synthetic microphone, real recorder and encrypted delivery without autoplay', async ({ browser, browserName }) => {
  test.skip(browserName !== 'chromium', 'Synthetic device fixture is Chromium-only; physical microphone acceptance is separate');
  const room = await conversation(browser);
  try {
    await room.guest.getByRole('button', { name: 'Записать голосовое', exact: true }).click();
    await expect(room.guest.getByRole('button', { name: 'Остановить запись', exact: true })).toBeVisible();
    await room.guest.waitForTimeout(1200); // Collect actual encoded synthetic audio, not a transport/crypto mock.
    await room.guest.getByRole('button', { name: 'Остановить запись', exact: true }).click();
    await expect(room.guest.getByRole('button', { name: 'Отправить голосовое', exact: true })).toBeEnabled(); await room.guest.getByRole('button', { name: 'Отправить голосовое', exact: true }).click();
    await reveal(room.creator); await expect(room.creator.getByRole('button', { name: 'Сохранить файл', exact: true })).toBeEnabled();
    const audio = room.creator.locator('audio'); await expect(audio).toHaveCount(1); expect(await audio.evaluate(node => node.paused && !node.autoplay)).toBe(true);
    expect((await downloadBytes(room.creator)).length > 44).toBe(true);
  } finally { await room.close(); }
});
