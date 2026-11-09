import { createHash } from 'node:crypto';
import { lstatSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

export const WEB_ARCHIVE = 'ssc-web.tar.gz';
export const allowedWebPath = name => name === 'index.html'
  || /^assets\/[A-Za-z0-9_.-]+\.(js|css|woff2)$/.test(name)
  || /^protocol\/[a-f0-9]{64}\/ssc_protocol(?:_bg\.wasm|\.js)$/.test(name);

export function protocolUrl(source) {
  const file = join(source, 'protocol-url.txt');
  const info = lstatSync(file);
  if (!info.isFile() || info.size > 128) throw new Error('Invalid protocol build metadata');
  const value = readFileSync(file, 'utf8').trim();
  if (!/^\/protocol\/[a-f0-9]{64}\/ssc_protocol\.js$/.test(value)) throw new Error('Invalid protocol build metadata');
  return value;
}

export function collectWebFiles(source) {
  if (!lstatSync(source).isDirectory()) throw new Error('Invalid web build directory');
  const files = [];
  let total = 0;
  const walk = (relative = '') => {
    for (const entry of readdirSync(join(source, relative), { withFileTypes: true })) {
      const name = relative ? `${relative}/${entry.name}` : entry.name;
      const info = lstatSync(join(source, name));
      if (info.isDirectory()) {
        if (!['assets', 'protocol'].includes(name) && !/^protocol\/[a-f0-9]{64}$/.test(name)) throw new Error('Unexpected web build directory');
        walk(name);
      } else if (name === 'protocol-url.txt') {
        protocolUrl(source); // This local build metadata is never published.
      } else {
        if (!info.isFile() || !allowedWebPath(name) || info.size === 0 || info.size > 8 * 1024 ** 2) throw new Error('Unexpected web build file');
        files.push(name); total += info.size;
        if (files.length > 512 || total > 32 * 1024 ** 2) throw new Error('Oversized web build');
      }
    }
  };
  walk(); files.sort();
  const url = protocolUrl(source);
  const protocol = files.filter(name => name.startsWith('protocol/'));
  if (!files.includes('index.html') || !files.some(name => /^assets\/.+\.js$/.test(name))
      || JSON.stringify(protocol) !== JSON.stringify([url.slice(1), url.slice(1).replace(/\.js$/, '_bg.wasm')].sort())) throw new Error('Incomplete web build');
  return files;
}

export function packageWeb({ source, output }) {
  source = resolve(source); output = resolve(output);
  const files = collectWebFiles(source);
  mkdirSync(output, { recursive: true });
  const archive = join(output, WEB_ARCHIVE);
  const result = spawnSync('tar', ['--format=ustar', '--sort=name', '--mtime=@0', '--owner=0', '--group=0', '--numeric-owner', '--mode=u=rw,go=r', '-czf', archive, '-C', source, '--', ...files], { encoding: 'utf8', maxBuffer: 64 * 1024 });
  if (result.error || result.status !== 0) throw new Error('Web archive creation failed');
  if (statSync(archive).size > 16 * 1024 ** 2) throw new Error('Oversized web archive');
  const hash = createHash('sha256').update(readFileSync(archive)).digest('hex');
  writeFileSync(`${archive}.sha256`, `${hash}  ${basename(archive)}\n`, { mode: 0o644 });
  return { archive, hash };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    if (process.argv.length !== 4) throw new Error('Expected source and output directories');
    packageWeb({ source: process.argv[2], output: process.argv[3] });
    process.stdout.write('Public web archive and checksum ready.\n');
  } catch {
    process.stderr.write('Web packaging rejected the build layout or archive.\n');
    process.exitCode = 1;
  }
}
