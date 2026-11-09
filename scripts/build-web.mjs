import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';

function run(command, args, env = process.env) {
  const result = spawnSync(command, args, { stdio: 'inherit', env });
  if (result.error || result.status !== 0) process.exit(result.status || 1);
}
const generated = resolve('web/public/protocol');
mkdirSync(generated, { recursive: true });
run('bash', ['crates/ssc-protocol/build-wasm.sh', generated]);
const files = ['ssc_protocol.js', 'ssc_protocol_bg.wasm'];
const digest = createHash('sha256');
for (const file of files) digest.update(readFileSync(resolve(generated, file)));
const version = digest.digest('hex');
for (const name of readdirSync(generated)) if (/^[a-f0-9]{64}$/.test(name)) rmSync(resolve(generated, name), { recursive: true });
mkdirSync(resolve(generated, version));
for (const file of files) copyFileSync(resolve(generated, file), resolve(generated, version, file));
const moduleUrl = `/protocol/${version}/ssc_protocol.js`;
run(process.execPath, ['node_modules/typescript/bin/tsc', '--project', 'web/tsconfig.json']);
run(process.execPath, ['node_modules/vite/bin/vite.js', 'build', '--config', 'web/vite.config.ts'], { ...process.env, VITE_SSC_PROTOCOL_URL: moduleUrl });
// Keep the unversioned development module out of the production archive.
for (const file of readdirSync('web/dist/protocol')) if (!/^[a-f0-9]{64}$/.test(file)) rmSync(resolve('web/dist/protocol', file), { recursive: true });
writeFileSync('web/dist/protocol-url.txt', moduleUrl + '\n');
