import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { build, validateBootstrap } from './build-personal.mjs';

function fixture(t) {
  const base=mkdtempSync(join(tmpdir(),'ssc-installer-test-'));
  t.after(()=>rmSync(base,{recursive:true,force:true}));
  const payload=join(base,'payload');mkdirSync(payload);
  writeFileSync(join(payload,'ssc.AppImage'),'#!/bin/sh\nprintf "fake-app:%s\\n" "$APPIMAGE_EXTRACT_AND_RUN"\n',{mode:0o700});
  const profile=join(base,'bootstrap.json');
  const data={version:1,helper_origin:'https://connect.vovakirdan.ru',bootstrap_token:Buffer.alloc(32,17).toString('base64url'),expires_at:new Date(Date.now()+3600000).toISOString()};
  writeFileSync(profile,JSON.stringify(data),{mode:0o600});
  return {base,payload,profile,data};
}
test('rejects unsafe, expired, noncanonical or foreign bootstrap input without exposing it',t=>{
  const f=fixture(t);assert.equal(validateBootstrap(f.profile).version,1);
  for(const change of [{helper_origin:'https://evil.example'},{expires_at:'2000-01-01T00:00:00Z'},{bootstrap_token:'bad'},{version:2},{extra:'ignored'}]){
    writeFileSync(f.profile,JSON.stringify({...f.data,...change}));assert.throws(()=>validateBootstrap(f.profile));
  }
  writeFileSync(f.profile,JSON.stringify(f.data));chmodSync(f.profile,0o644);assert.throws(()=>validateBootstrap(f.profile));
});
test('Linux personal installer installs, updates safely and uninstalls while retaining access',t=>{
  if(process.platform!=='linux'||process.getuid()===0){t.skip('requires a non-root Linux user');return;}
  const f=fixture(t),output=join(f.base,'setup.run'),home=join(f.base,'home with spaces');mkdirSync(home);
  const env={...process.env,HOME:home,XDG_DATA_HOME:join(home,'.local/share')};delete env.SSC_PROFILE_DIR;
  build({platform:'linux',payload:f.payload,bootstrap:f.profile,output});
  const install=()=>{const r=spawnSync(output,[],{env,encoding:'utf8'});assert.equal(r.status,0,r.stderr);assert.ok(!r.stdout.includes(f.data.bootstrap_token));};
  install();
  const profile=join(env.XDG_DATA_HOME,'com.vladimirkirdan.ssc'),app=join(env.XDG_DATA_HOME,'ssc/app');
  assert.equal(statSync(profile).mode&0o777,0o700);assert.equal(statSync(join(profile,'bootstrap.json')).mode&0o777,0o600);
  assert.equal(JSON.parse(readFileSync(join(profile,'bootstrap.json'))).bootstrap_token,f.data.bootstrap_token);
  const launch=spawnSync(join(app,'launch'),[],{env,encoding:'utf8'});assert.equal(launch.status,0);assert.equal(launch.stdout.trim(),'fake-app:1');
  writeFileSync(join(profile,'access.key'),'private-key',{mode:0o600});
  writeFileSync(join(profile,'access.active'),'active',{mode:0o600});rmSync(join(profile,'bootstrap.json'));
  install();assert.equal(readFileSync(join(profile,'access.key'),'utf8'),'private-key');assert.equal(existsSync(join(profile,'bootstrap.json')),false);
  const uninstall=spawnSync(join(app,'uninstall'),[],{env,encoding:'utf8'});assert.equal(uninstall.status,0,uninstall.stderr);
  assert.equal(existsSync(join(app,'ssc.AppImage')),false);assert.ok(existsSync(join(profile,'access.key')));
  assert.equal(existsSync(join(env.XDG_DATA_HOME,'applications/ssc.desktop')),false);
});
test('NSIS template compiles with fixture payloads and rejects script metacharacters',t=>{
  const compiler=process.env.MAKENSIS||'/usr/bin/makensis';
  if(!existsSync(compiler)){t.skip('NSIS is checked where the compiler is installed');return;}
  const f=fixture(t);writeFileSync(join(f.payload,'ssc.exe'),'fixture');writeFileSync(join(f.payload,'MicrosoftEdgeWebview2Setup.exe'),'fixture');
  assert.throws(()=>build({platform:'windows',payload:f.payload,bootstrap:f.profile,output:join(f.base,'installer $literal.exe'),compiler}));
  const output=join(f.base,'installer setup.exe');
  build({platform:'windows',payload:f.payload,bootstrap:f.profile,output,compiler});
  assert.equal(readFileSync(output).subarray(0,2).toString(),'MZ');
});
