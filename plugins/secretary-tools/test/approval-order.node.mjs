// Real locked SDK approval ordering + built plugin, with synthetic Core replies.
// Native factory resolution has its own suite; this isolates real approval ordering.
import {test,after} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,dirname} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';

const state=await mkdtemp(join(tmpdir(),'ciel-native-approval-order-'));
const saved=new Map(['OPENCLAW_STATE_DIR','OPENCLAW_CONFIG_PATH','SECRETARY_CORE_TOKEN','SECRETARY_APPROVAL_TOKEN'].map(key=>[key,process.env[key]]));
process.env.OPENCLAW_STATE_DIR=state;process.env.OPENCLAW_CONFIG_PATH=join(state,'config.json');
process.env.SECRETARY_CORE_TOKEN='synthetic-core';process.env.SECRETARY_APPROVAL_TOKEN='synthetic-approval';
await writeFile(process.env.OPENCLAW_CONFIG_PATH,JSON.stringify({plugins:{enabled:false},logging:{level:'silent'}}));
after(async()=>{
 for(const [key,value] of saved){if(value===undefined)delete process.env[key];else process.env[key]=value;}
 await rm(state,{recursive:true,force:true});
});

async function sdkRoot() {
 let path=dirname(fileURLToPath(import.meta.resolve('openclaw/plugin-sdk/plugin-entry')));
 for(let depth=0;depth<5;depth++,path=dirname(path)) {
  try {
   const pkg=JSON.parse(await readFile(join(path,'package.json'),'utf8'));
   if(pkg.name==='openclaw'){assert.equal(pkg.version,'2026.10.7');return path;}
  } catch(error) {if(error?.code!=='ENOENT')throw error;}
 }
 throw Error('Pinned SDK package root not found');
}
const root=await sdkRoot();
const sdk=await import(pathToFileURL(join(root,'dist/agent-tools.before-tool-call-CReBJLEA.mjs')).href);
const embedded=await import(pathToFileURL(join(root,'dist/embedded-mode--Mk71zY-.mjs')).href);
const {default:entry}=await import('../dist/index.js');
assert.equal(typeof sdk.K,'function');assert.equal(typeof sdk.q,'function');

function deferred(){let resolve;const promise=new Promise(done=>{resolve=done;});return {promise,resolve};}
const params={operation_id:'op'};
const change={...params,spreadsheet:'sheet',view:'grid',action:'update',fields:{range:'A1:A1',values:[['new']]}};
const preview={...change,resource_kind:'sheets_grid',actor:'owner',record_id:'physical:7',before:[[{stringValue:'old'}]],after:[[{stringValue:'new'}]],mapping_version:'v1',source_revision:'r1',preview_digest:'captured-digest',expires_at:'2099-01-01T00:00:00Z',state:'previewed'};

async function fixture(decision='allow-once') {
 const originalFetch=globalThis.fetch;const paths=[];const ack=deferred();const callback=deferred();let current=true;
 globalThis.fetch=async(input,init)=>{
  const request=new Request(input,init);const url=new URL(request.url);assert.equal(url.origin,'http://127.0.0.1:8787');paths.push(url.pathname);
  if(url.pathname.endsWith('/sheets_preview_change'))return Response.json({ok:true,data:preview});
  if(url.pathname.endsWith('/approval'))return ack.promise;
  if(url.pathname.endsWith('/sheets_update'))return Response.json({ok:true,data:{state:'applied'}});
  throw Error('Unexpected Core route');
 };
 const tools=new Map();let before;
 const api={pluginConfig:{owner:'owner',approvalTransportQualified:true},runtime:{gateway:{isAvailable:()=>false}},registerTool(factory,options){tools.set(options.name,factory);},on(name,handler){if(name==='before_tool_call')before=handler;}};
 const abort=new AbortController();const instances=new Map();
 const context={agentId:'agent',sessionKey:'agent:owner',sessionId:'incarnation',requesterSenderId:'owner',senderIsOwner:true,assertInvocationCurrent(){if(!current)throw Error('INVOCATION_EXPIRED');}};
 const broker=new sdk.q();embedded.n(true);sdk.Y(broker);
 broker.subscribe(event=>{if(event.event==='plugin.approval.requested')queueMicrotask(()=>broker.resolve(event.payload.id,decision));});
 async function run(name,input){const tool=instances.get(name);assert(tool);return await tool.execute('synthetic',input,abort.signal);}
 try {
  entry.register(api);for(const [name,factory] of tools){assert.equal(factory.contextVersion,2);instances.set(name,factory.create(context));}assert.equal((await run('sheets_preview_change',change)).details.ok,true);
  assert.equal(typeof before,'function');
  const result=await before({toolName:'sheets_update',params},{...context,abortSignal:abort.signal,requester:{senderId:'owner',senderIsOwner:true}});
  assert(result.requireApproval);
  const original=result.requireApproval;
  const approval={...original,pluginId:'secretary-tools',onResolution(value){const completion=original.onResolution(value);completion.then(()=>callback.resolve({ok:true}),error=>callback.resolve({ok:false,error}));return completion;}};
  const native=await sdk.K({deferredApproval:{approval,toolName:'sheets_update',toolCallId:'synthetic',baseParams:params},signal:new AbortController().signal});
  return {native,paths,ack,callback,run,revoke(){current=false;},close(){broker.stop();sdk.J(broker);embedded.n(false);globalThis.fetch=originalFetch;}};
 } catch(error){broker.stop();sdk.J(broker);embedded.n(false);globalThis.fetch=originalFetch;throw error;}
}

test('actual SDK returns allow before acknowledgement; built plugin still waits before apply',{timeout:20000},async()=>{
 const f=await fixture();try {
  assert.equal(f.native.blocked,false);let finished=false;
  const write=f.run('sheets_update',params).then(value=>{finished=true;return value;});
  await new Promise(resolve=>setImmediate(resolve));
  assert.equal(finished,false);assert.deepEqual(f.paths,['/v1/tools/sheets_preview_change','/v1/operations/op/approval']);
  f.ack.resolve(Response.json({ok:true,data:{state:'approved'}}));assert.equal((await f.callback.promise).ok,true);
  assert.equal((await write).details.data.state,'applied');assert.equal(f.paths.filter(path=>path.endsWith('/sheets_update')).length,1);
 } finally {f.close();}
});

test('actual SDK allowed outcome cannot conceal a refused Core approval',{timeout:20000},async()=>{
 const f=await fixture();try {
  assert.equal(f.native.blocked,false);const write=f.run('sheets_update',params);
  f.ack.resolve(Response.json({ok:false,error:{code:'operation_conflict'}},{status:409}));
  assert.equal((await f.callback.promise).ok,false);const result=await write;
  assert.equal(result.details.error.code,'APPROVAL_REJECTED');assert.equal(f.paths.length,2);
 } finally {f.close();}
});

test('built factory checks current invocation after a delayed native acknowledgement',{timeout:20000},async()=>{
 const f=await fixture();try {
  assert.equal(f.native.blocked,false);const write=f.run('sheets_update',params);
  await new Promise(resolve=>setImmediate(resolve));f.revoke();f.ack.resolve(Response.json({ok:true,data:{state:'approved'}}));
  const result=await write;assert.equal(result.details.error.code,'INVOCATION_EXPIRED');assert.equal(f.paths.length,2);
 } finally {f.close();}
});

test('actual native denial persists only rejection and never starts apply',{timeout:20000},async()=>{
 const f=await fixture('deny');try {
  assert.equal(f.native.blocked,true);f.ack.resolve(Response.json({ok:true,data:{state:'rejected'}}));
  assert.equal((await f.callback.promise).ok,true);assert.equal(f.paths.length,2);
 } finally {f.close();}
});
