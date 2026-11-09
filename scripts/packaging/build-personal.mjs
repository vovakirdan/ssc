import { chmodSync, copyFileSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const here = dirname(fileURLToPath(import.meta.url));
const origin = 'https://connect.vovakirdan.ru';
function fail() { throw new Error('Invalid personal package input'); }
export function validateBootstrap(path) {
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 4096) fail();
  if (process.platform !== 'win32' && (stat.mode & 0o077)) fail();
  const profile = JSON.parse(readFileSync(path, 'utf8'));
  if (Object.keys(profile).sort().join(',') !== 'bootstrap_token,expires_at,helper_origin,version' || profile.version !== 1 || profile.helper_origin !== origin ||
      !/^[A-Za-z0-9_-]{43}$/.test(profile.bootstrap_token) || Buffer.from(profile.bootstrap_token, 'base64url').length !== 32 ||
      Buffer.from(profile.bootstrap_token, 'base64url').toString('base64url') !== profile.bootstrap_token ||
      typeof profile.expires_at !== 'string' || !Number.isFinite(Date.parse(profile.expires_at)) || Date.parse(profile.expires_at) <= Date.now() || Date.parse(profile.expires_at) > Date.now() + 8 * 86400000) fail();
  return profile;
}
function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: ['ignore', 'pipe', 'pipe'], ...options });
  // Compiler output can contain generated data: never relay it to CI logs.
  if (result.error || result.status !== 0) throw new Error('Personal package tool failed');
  return result.stdout;
}
function regular(path) { const s = lstatSync(path); if (!s.isFile() || s.isSymbolicLink()) fail(); }
export function build({ platform, payload, bootstrap, output, version = '0.2.0', compiler = 'makensis' }) {
  if (!['linux', 'windows'].includes(platform) || !/^\d+\.\d+\.\d+$/.test(version)) fail();
  validateBootstrap(bootstrap);
  payload = resolve(payload); output = resolve(output);
  if (/["$\r\n]/.test(output)) fail();
  try { lstatSync(output); fail(); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  const app = join(payload, platform === 'linux' ? 'ssc.AppImage' : 'ssc.exe');
  regular(app);
  mkdirSync(dirname(output), { recursive: true, mode: 0o700 });
  const previousMask = process.umask(0o077);
  const work = mkdtempSync(join(tmpdir(), 'ssc-package-')); chmodSync(work, 0o700);
  let complete = false;
  try {
    copyFileSync(bootstrap, join(work, 'bootstrap.json')); chmodSync(join(work, 'bootstrap.json'), 0o600);
    copyFileSync(app, join(work, basename(app)));
    if (platform === 'linux') {
      chmodSync(join(work, 'ssc.AppImage'), 0o700);
      copyFileSync(join(here, 'templates', 'install-linux.sh'), join(work, 'install.sh'));
      const archive = run('tar', ['-czf', '-', '--', 'install.sh', 'ssc.AppImage', 'bootstrap.json'], { cwd: work, maxBuffer: 512 * 1024 * 1024 });
      const header = readFileSync(join(here, 'templates', 'self-extract.sh'), 'utf8');
      writeFileSync(output, Buffer.concat([Buffer.from(header), archive]), { mode: 0o700 });
    } else {
      const webview = join(payload, 'MicrosoftEdgeWebview2Setup.exe'); regular(webview);
      copyFileSync(webview, join(work, 'MicrosoftEdgeWebview2Setup.exe'));
      copyFileSync(join(here, 'templates', 'protect-profile.ps1'), join(work, 'protect-profile.ps1'));
      const escape = s => s.replaceAll('$', () => '$$').replaceAll('"', () => '$\\"');
      const script = readFileSync(join(here, 'templates', 'personal.nsi'), 'utf8').replaceAll('@@OUTPUT@@', () => escape(output)).replaceAll('@@VERSION@@', version);
      writeFileSync(join(work, 'personal.nsi'), script);
      run(compiler, ['-V2', 'personal.nsi'], { cwd: work, maxBuffer: 4 * 1024 * 1024 });
      if (!existsSync(output)) throw new Error('Missing personal installer');
      chmodSync(output, 0o600);
    }
    complete = true;
  } finally {
    rmSync(work, { recursive: true, force: true });
    if (!complete) rmSync(output, { force: true });
    process.umask(previousMask);
  }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = {};
    const args = process.argv.slice(2);
    for (let i = 0; i < args.length; i += 2) {
      if (!['--platform','--payload','--bootstrap','--output','--version','--compiler'].includes(args[i]) || !args[i + 1]) fail();
      options[args[i].slice(2)] = args[i + 1];
    }
    build(options);
    process.stdout.write('Personal installer created.\n');
  } catch { process.stderr.write('Personal installer creation failed; no activation data was logged.\n'); process.exitCode = 1; }
}
