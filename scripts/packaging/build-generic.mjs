import { copyFileSync, createReadStream, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
const [platform, app, destination, webview] = process.argv.slice(2);
if (!['linux','windows'].includes(platform) || !app || !destination || (platform==='windows'&&!webview)) throw new Error('Expected platform app destination [webview]');
const work=mkdtempSync(join(tmpdir(),'ssc-generic-'));
try {
  const files=[platform==='linux'?'ssc.AppImage':'ssc.exe'];
  copyFileSync(app,join(work,files[0]));
  if(platform==='windows'){ files.push('MicrosoftEdgeWebview2Setup.exe');copyFileSync(webview,join(work,files[1])); }
  mkdirSync(destination,{recursive:true});
  const name=`ssc-generic-${platform}-x64.tar.gz`,output=resolve(destination,name);
  const packed=spawnSync('tar',['-czf',output,'--',...files],{cwd:work,stdio:'inherit'});
  if(packed.error||packed.status!==0) throw new Error('Generic archive failed');
  const hash=createHash('sha256');for await(const chunk of createReadStream(output))hash.update(chunk);
  writeFileSync(`${output}.sha256`,`${hash.digest('hex')}  ${name}\n`);
} finally { rmSync(work,{recursive:true,force:true}); }
