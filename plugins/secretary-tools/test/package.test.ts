import {test,expect} from 'bun:test';
import {coreTools} from '../src/bridge';

test('manifest declares ownership of every secretary tool before runtime loading', async()=>{
 const manifest=await Bun.file(new URL('../openclaw.plugin.json',import.meta.url)).json();
 const expected=[...coreTools,...['create','list','get','reschedule','pause','resume','cancel','history'].map(action=>`reminders_${action}`),'memory_capabilities'];
 expect(manifest.contracts?.tools?.toSorted()).toEqual(expected.toSorted());
 expect(manifest.activation?.onStartup).toBe(true);
 expect(manifest.configSchema.properties.approvalTransportQualified.default).toBe(false);
 expect(manifest.configSchema.properties.schedulerQualified.default).toBe(false);
});

test('package metadata check rejects tool-contract drift without loading the host SDK',async()=>{
 const {mkdtemp,writeFile,rm}=await import('node:fs/promises');
 const {tmpdir}=await import('node:os');
 const {join}=await import('node:path');
 const root=await mkdtemp(join(tmpdir(),'secretary-package-'));
 try {
  const manifest=await Bun.file(new URL('../openclaw.plugin.json',import.meta.url)).json();
  const pkg=await Bun.file(new URL('../package.json',import.meta.url)).json();
  await writeFile(join(root,'package.json'),JSON.stringify(pkg));
  const checker=new URL('../scripts/check-package.ts',import.meta.url).pathname;
  manifest.contracts.tools.push('external_send');
  await writeFile(join(root,'openclaw.plugin.json'),JSON.stringify(manifest));
  const bad=Bun.spawnSync([process.execPath,checker,'--root',root]);
  expect(bad.exitCode).toBe(1);
  expect(bad.stderr.toString()).toContain('TOOL_CONTRACT_MISMATCH');
  manifest.contracts.tools.pop();
  await writeFile(join(root,'openclaw.plugin.json'),JSON.stringify(manifest));
  const good=Bun.spawnSync([process.execPath,checker,'--root',root]);
  expect(good.exitCode).toBe(0);
  expect(good.stdout.toString()).toContain('metadata only');
  pkg.files=['**'];
  await writeFile(join(root,'package.json'),JSON.stringify(pkg));
  const broad=Bun.spawnSync([process.execPath,checker,'--root',root]);
  expect(broad.exitCode).toBe(1);
  expect(broad.stderr.toString()).toContain('PACKAGE_FILE_ALLOWLIST_REQUIRED');
 } finally {await rm(root,{recursive:true,force:true});}
});

test('package file selection excludes source, operator configuration and credential files',async()=>{
 const {mkdtemp,writeFile,mkdir,rm}=await import('node:fs/promises');
 const {tmpdir}=await import('node:os');
 const {join}=await import('node:path');
 const {spawnSync}=await import('node:child_process');
 const root=await mkdtemp(join(tmpdir(),'secretary-pack-'));
 try {
  const pkg=await Bun.file(new URL('../package.json',import.meta.url)).json();
  await mkdir(join(root,'dist'));await mkdir(join(root,'config'));await mkdir(join(root,'src'));
  await writeFile(join(root,'package.json'),JSON.stringify(pkg));
  await writeFile(join(root,'openclaw.plugin.json'),'{}');
  await writeFile(join(root,'README.md'),'Synthetic packaging fixture');
  await writeFile(join(root,'dist/index.js'),'export default {};');
  await writeFile(join(root,'google-service-account.json'),'SYNTHETIC_DO_NOT_SHIP');
  await writeFile(join(root,'config/operator.env'),'SYNTHETIC_DO_NOT_SHIP');
  await writeFile(join(root,'src/private-notes.txt'),'SYNTHETIC_DO_NOT_SHIP');
  await writeFile(join(root,'user.npmrc'),'');await writeFile(join(root,'global.npmrc'),'');
  const packed=spawnSync('npm',['pack','--dry-run','--json','--ignore-scripts','--offline','--workspaces=false'],{
   cwd:root,encoding:'utf8',timeout:10000,
   env:{...process.env,NPM_CONFIG_CACHE:join(root,'cache'),NPM_CONFIG_USERCONFIG:join(root,'user.npmrc'),NPM_CONFIG_GLOBALCONFIG:join(root,'global.npmrc')},
  });
  expect(packed.status).toBe(0);
  const paths=JSON.parse(packed.stdout)[0].files.map((file:{path:string})=>file.path).sort();
  expect(paths).toEqual(['README.md','dist/index.js','openclaw.plugin.json','package.json']);
 } finally {await rm(root,{recursive:true,force:true});}
});
