// Shared actual Core fixture preparation; only provider/source IO is controlled.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {readFile} from 'node:fs/promises';
import {dirname,join} from 'node:path';
import {fileURLToPath} from 'node:url';
const project=fileURLToPath(new URL('../../../../',import.meta.url));
// Preparation may need a cold compile. Keep its budget separate from the Core's
// eight-second dispatch handshake and align target selection with verify.sh.
const BUILD_TIMEOUT_MS=180000;
async function output(command,args,options={}) {
 const child=spawn(command,args,{cwd:project,...options});let text='',error='',overflow=false,timedOut=false;
 child.stdout.on('data',chunk=>{text+=chunk;if(text.length>1048576){overflow=true;child.kill('SIGKILL');}});
 child.stderr.on('data',chunk=>{error+=chunk;if(error.length>32768){overflow=true;child.kill('SIGKILL');}});
 const timer=setTimeout(()=>{timedOut=true;child.kill('SIGKILL');},BUILD_TIMEOUT_MS);
 const code=await new Promise((resolve,reject)=>{child.once('close',resolve);child.once('error',reject);}).finally(()=>clearTimeout(timer));
 assert.equal(overflow,false,'Fixture compiler output too large');
 assert.equal(timedOut,false,'Synthetic fixture compilation exceeded its build budget: '+error);
 assert.equal(code,0,'Synthetic fixture build failed: '+error);return text;
}
export async function startTaskCoreFixture(){
 const built=await output('cargo',['test','--offline','--locked','--workspace','--no-run','--message-format=json']);
 const artifact=built.trim().split('\n').map(line=>JSON.parse(line)).find(event=>event.reason==='compiler-artifact' && event.target.name==='task_native' && event.executable);
 assert(artifact);const child=spawn(artifact.executable,['--ignored','--nocapture','native_sdk_fixture_server'],{cwd:project,env:{...process.env,CIEL_TASK_NATIVE_FIXTURE:'synthetic'}});
 let diagnostics='';child.stderr.on('data',chunk=>{diagnostics+=chunk;if(diagnostics.length>32768)child.kill('SIGKILL');});
 const exit=new Promise((resolve,reject)=>{child.once('exit',resolve);child.once('error',reject);});
 try{
  const url=await new Promise((resolve,reject)=>{
   let text='';const timer=setTimeout(()=>reject(Error('Synthetic Core startup timed out')),15000);
   child.stdout.on('data',chunk=>{text+=chunk;const found=text.match(/CIEL_TASK_NATIVE_LISTENING (http:\/\/127\.0\.0\.1:\d+)/);if(found){clearTimeout(timer);resolve(found[1]);}else if(text.length>32768){clearTimeout(timer);reject(Error('Unexpected fixture output'));}});
   child.once('exit',()=>{clearTimeout(timer);reject(Error('Synthetic Core exited before startup: '+diagnostics));});
  });
  async function fixture(action){const response=await fetch(url+'/__fixture',{method:'POST',body:JSON.stringify({action}),headers:{'content-type':'application/json'},signal:AbortSignal.timeout(5000)});assert(response.ok);return response.json();}
  return {url,fixture,async close(){let timer;try{await fixture('stop');const code=await Promise.race([exit,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Synthetic Core stop timed out')),5000);})]);assert.equal(code,0,diagnostics);}catch(error){child.kill('SIGKILL');await exit;throw error;}finally{clearTimeout(timer);}}};
 }catch(error){child.kill('SIGKILL');await exit;throw error;}
}
export async function taskSdkRoot(){let path=dirname(fileURLToPath(import.meta.resolve('openclaw/plugin-sdk/plugin-entry')));for(let depth=0;depth<5;depth++,path=dirname(path)){try{const pkg=JSON.parse(await readFile(join(path,'package.json'),'utf8'));if(pkg.name==='openclaw'){assert.equal(pkg.version,'2026.10.7');return path;}}catch(error){if(error?.code!=='ENOENT')throw error;}}throw Error('Pinned SDK not found');}
