import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, truncateSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { allowedWebPath, collectWebFiles, packageWeb, protocolUrl, WEB_ARCHIVE } from './build-web.mjs';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'ssc-web-package-')); t.after(() => rmSync(root, { recursive: true, force: true }));
  const source = join(root, 'build'), output = join(root, 'output'), hash = 'a'.repeat(64);
  mkdirSync(join(source, 'assets'), { recursive: true }); mkdirSync(join(source, 'protocol', hash), { recursive: true });
  writeFileSync(join(source, 'index.html'), '<!doctype html><script src="/assets/app.js"></script>');
  writeFileSync(join(source, 'assets', 'app.js'), 'console.log("fixture")'); writeFileSync(join(source, 'assets', 'app.css'), 'body{}');
  writeFileSync(join(source, 'protocol', hash, 'ssc_protocol.js'), 'export default()=>{}');
  writeFileSync(join(source, 'protocol', hash, 'ssc_protocol_bg.wasm'), Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
  writeFileSync(join(source, 'protocol-url.txt'), `/protocol/${hash}/ssc_protocol.js\n`);
  return { source, output, hash };
}

test('web archive contains only checked regular public files and its checksum matches', t => {
  const f = fixture(t); assert.equal(protocolUrl(f.source), `/protocol/${f.hash}/ssc_protocol.js`); const expected = collectWebFiles(f.source); const result = packageWeb(f);
  const listed = spawnSync('tar', ['-tzf', result.archive], { encoding: 'utf8' }); assert.equal(listed.status, 0);
  assert.deepEqual(listed.stdout.trim().split('\n'), expected); assert.equal(expected.length, 5);
  assert.ok(!listed.stdout.includes('protocol-url.txt'));
  const checksum = createHash('sha256').update(readFileSync(result.archive)).digest('hex'); assert.equal(checksum, result.hash);
  assert.equal(readFileSync(`${result.archive}.sha256`, 'utf8'), `${checksum}  ${WEB_ARCHIVE}\n`);
  assert.equal(packageWeb(f).hash, result.hash, 'repacking must preserve the public artifact digest');
});

test('unexpected secrets, source maps, symlinks and oversized assets reject packaging', t => {
  const f = fixture(t);
  for (const name of ['bootstrap.json', 'access.key', 'assets/app.js.map', 'protocol/ssc_protocol.js']) {
    const target = join(f.source, name); writeFileSync(target, 'private fixture');
    assert.throws(() => collectWebFiles(f.source)); rmSync(target);
  }
  symlinkSync(join(f.source, 'index.html'), join(f.source, 'assets', 'outside.js')); assert.throws(() => collectWebFiles(f.source)); rmSync(join(f.source, 'assets', 'outside.js'));
  const target = join(f.source, 'assets', 'huge.js'); writeFileSync(target, ''); truncateSync(target, 8 * 1024 ** 2 + 1); assert.throws(() => collectWebFiles(f.source));
});

test('protocol metadata must name the sole versioned JS/WASM pair', t => {
  const f = fixture(t);
  for (const value of ['https://evil.example/protocol.js', '/protocol/../ssc_protocol.js', '/protocol/ssc_protocol.js', `/protocol/${'b'.repeat(64)}/ssc_protocol.js`]) {
    writeFileSync(join(f.source, 'protocol-url.txt'), value); assert.throws(() => collectWebFiles(f.source));
  }
  for (const name of ['/index.html', '../index.html', 'assets/../index.html', 'assets/data.json', 'protocol/a/ssc_protocol.js', 'protocol/' + 'a'.repeat(64) + '/ssc_protocol.d.ts']) assert.equal(allowedWebPath(name), false);
});
