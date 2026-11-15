// Actual pinned SDK registry, context issuer, callback wrapper and run-scope fence.
// Requester identity/source invalidation are fixture inputs, not live owner routing.
import {test,after} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,dirname} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';

const state=await mkdtemp(join(tmpdir(),'ciel-native-v2-'));
const saved=new Map(['OPENCLAW_STATE_DIR','OPENCLAW_CONFIG_PATH','SECRETARY_CORE_TOKEN','SECRETARY_APPROVAL_TOKEN'].map(key=>[key,process.env[key]]));
process.env.OPENCLAW_STATE_DIR=state;process.env.OPENCLAW_CONFIG_PATH=join(state,'config.json');
process.env.SECRETARY_CORE_TOKEN='synthetic-core';process.env.SECRETARY_APPROVAL_TOKEN='synthetic-approval';
await writeFile(process.env.OPENCLAW_CONFIG_PATH,JSON.stringify({plugins:{enabled:false},logging:{level:'silent'}}));
after(async()=>{for(const [key,value] of saved){if(value===undefined)delete process.env[key];else process.env[key]=value;}await rm(state,{recursive:true,force:true});});

async function sdkRoot(){
 let path=dirname(fileURLToPath(import.meta.resolve('openclaw/plugin-sdk/plugin-entry')));
 for(let depth=0;depth<5;depth++,path=dirname(path)){
  try {const pkg=JSON.parse(await readFile(join(path,'package.json'),'utf8'));if(pkg.name==='openclaw'){assert.equal(pkg.version,'2026.10.7');return path;}}
  catch(error){if(error?.code!=='ENOENT')throw error;}
 }
 throw Error('Pinned SDK package root not found');
}
const root=await sdkRoot(),load=async name=>import(pathToFileURL(join(root,'dist',name)).href);
const registrySdk=await load('registry-BwDJiK-z.mjs');
const toolsSdk=await load('tools-fou-nJCY.mjs');
const lifecycle=await load('registry-lifecycle-C978THT2.mjs');
const contextSdk=await load('load-context.resolve-COA_cf7U.mjs');
const guards=await load('agent-tool-source-execution-guard-DI8a3ZFX.mjs');
const globalHooks=await load('hook-runner-global-trQbJiN5.mjs');
const resetService=await load('session-reset-service-DN3eiupo.mjs');
const {default:entry,registerNativeSessionLifecycle}=await import('../dist/index.js');
const manifest=JSON.parse(await readFile(new URL('../openclaw.plugin.json',import.meta.url),'utf8'));
const logger={info(){},warn(){},error(message){throw Error(message);},debug(){}};

function fixture(pluginSettings={}){
 const config={plugins:{enabled:true,allow:['secretary-tools'],entries:{'secretary-tools':{enabled:true}}}};
 const runtime={config:{current:()=>config},gateway:{isAvailable:()=>false}};
 const builder=registrySdk.t({logger,runtime,coreGatewayMethodNames:[],activateGlobalSideEffects:false});
 const source=fileURLToPath(new URL('../dist/index.js',import.meta.url));
 const rootDir=dirname(dirname(source));
 const record={...manifest,source,rootDir,origin:'workspace',format:'bundle',enabled:true,status:'loaded',toolNames:[],hookNames:[],commandNames:[],gatewayMethods:[],cliRegistrars:[],serviceIds:[]};
 builder.registry.plugins.push(record);
 const api=builder.createApi(record,{config,pluginConfig:{owner:'owner',approvalTransportQualified:true,...pluginSettings},registrationMode:'full'});
 entry.register(api);
 assert.equal(builder.registry.diagnostics.length,0,JSON.stringify(builder.registry.diagnostics));
 assert.equal(builder.registry.tools.length,manifest.contracts.tools.length);
 assert(builder.registry.tools.every(tool=>tool.contextVersion===2));
 const plugin={...record,toolMetadata:{}};
 const metadataSnapshot={plugins:[plugin],byPluginId:new Map([[plugin.id,plugin]]),index:{plugins:[{pluginId:plugin.id,origin:plugin.origin,enabled:true}]}};
 const loadContext=contextSdk.t({config,env:process.env,workspaceDir:state,logger,manifestRegistry:{plugins:[plugin],diagnostics:[]}});
 const context={config,agentId:'agent',sessionKey:'agent:owner',sessionId:'incarnation',requesterSenderId:'owner',senderIsOwner:true,workspaceDir:state};
 function resolve(assertInvocationCurrent,extra={}){
  return toolsSdk.i({context:{...context,...extra},env:process.env,toolAllowlist:['secretary-tools'],assertInvocationCurrent,preparedRuntime:{loadContext,metadataSnapshot,registry:builder.registry}});
 }
 return {builder,api,config,context,resolve,close(){globalHooks.o();lifecycle.T(builder.registry);}};
}
function find(tools,name){const tool=tools.find(tool=>tool.name===name);assert(tool,`Missing ${name}`);return tool;}
function deferred(){let resolve;const promise=new Promise(done=>{resolve=done;});return {promise,resolve};}

test('actual Gateway new/reset session-end emitter revokes the exact old factory and leaves its successor current',async()=>{
 for(const reason of ['new','reset']){
  const f=fixture(),original=globalThis.fetch,observed=deferred();let sends=0;
  globalThis.fetch=async()=>{sends++;return Response.json({ok:true,data:[]});};
  try{
   // Keep the supplied run assertion live to isolate revocation by the real SDK
   // emitter/hook runner. These requester identities are still synthetic.
   const old=find(f.resolve(()=>{}),'sheets_list');
   const next=find(f.resolve(()=>{},{sessionId:'successor'}),'sheets_list');
   f.api.on('session_end',()=>{observed.resolve();});globalHooks.a(f.builder.registry);
   resetService.a({cfg:f.config,...f.context,reason,nextSessionId:'successor',storePath:join(state,'sessions.json')});
   await Promise.race([observed.promise,new Promise((_,reject)=>{const timer=setTimeout(()=>reject(Error('Session-end hook not dispatched')),3000);timer.unref();})]);
   assert.equal((await old.execute('old',{})).details.error.code,'INVOCATION_EXPIRED');assert.equal(sends,0);
   assert.equal((await find(f.resolve(()=>{}),'sheets_list').execute('reissued-old',{})).details.error.code,'INVOCATION_EXPIRED');assert.equal(sends,0);
   assert.equal((await next.execute('new',{})).details.ok,true);assert.equal(sends,1);
   // Delayed/duplicate old-incarnation delivery must not select the successor.
   await globalHooks.n().runSessionEnd({sessionId:'incarnation',sessionKey:f.context.sessionKey,messageCount:0,reason},f.context);
   assert.equal((await next.execute('still-current',{})).details.ok,true);assert.equal(sends,2);
  }finally{globalThis.fetch=original;f.close();}
 }
});

test('lifecycle registration is idempotent and contradictory session-end payloads cannot revoke a current factory',async()=>{
 const f=fixture(),original=globalThis.fetch;let sends=0;
 globalThis.fetch=async()=>{sends++;return Response.json({ok:true,data:[]});};
 try{
  registerNativeSessionLifecycle(f.api);registerNativeSessionLifecycle(f.api);
  assert.equal(f.builder.registry.typedHooks.filter(hook=>hook.hookName==='session_end').length,1);
  assert.equal(f.builder.registry.typedHooks.filter(hook=>hook.hookName==='before_reset').length,1);
  const tool=find(f.resolve(()=>{}),'sheets_list');globalHooks.a(f.builder.registry);
  for(const event of [{sessionId:'wrong',sessionKey:f.context.sessionKey},{sessionId:f.context.sessionId,sessionKey:'wrong'}]){
   await globalHooks.n().runSessionEnd({...event,messageCount:0,reason:'reset'},f.context);
   assert.equal((await tool.execute('valid',{})).details.ok,true);
  }
  assert.equal(sends,2);
 }finally{globalThis.fetch=original;f.close();}
});

test('actual Gateway before-reset emitter revokes a retained approval before a Core decision',async()=>{
 const f=fixture(),original=globalThis.fetch,observed=deferred(),paths=[];
 globalThis.fetch=async(input,init)=>{
  const request=new Request(input,init);paths.push(new URL(request.url).pathname);
  return Response.json({ok:true,data:{...await request.json(),resource_kind:'sheets_grid',actor:'owner',preview_digest:'synthetic-digest',expires_at:'2099-01-01T00:00:00Z',state:'previewed'}});
 };
 try{
  const tools=f.resolve(()=>{});
  await find(tools,'sheets_preview_change').execute('preview',{operation_id:'op',spreadsheet:'sheet',view:'grid',action:'update',fields:{range:'A1:A1',values:[['new']]}});
  const hook=f.builder.registry.typedHooks.find(hook=>hook.hookName==='before_tool_call');assert(hook);
  const decision=await hook.handler({toolName:'sheets_update',params:{operation_id:'op'}},{...f.context,toolName:'sheets_update',abortSignal:new AbortController().signal,requester:{senderId:'owner',senderIsOwner:true}});assert(decision.requireApproval);
  f.api.on('before_reset',()=>{observed.resolve();});globalHooks.a(f.builder.registry);
  await resetService.n({cfg:f.config,target:{agentId:f.context.agentId,canonicalKey:f.context.sessionKey},entry:{sessionId:f.context.sessionId},storePath:join(state,'sessions.json'),messages:[],reason:'reset'});
  await Promise.race([observed.promise,new Promise((_,reject)=>{const timer=setTimeout(()=>reject(Error('Before-reset hook not dispatched')),3000);timer.unref();})]);
  await assert.rejects(decision.requireApproval.onResolution('allow-once'),/INVOCATION_EXPIRED/);
  assert.deepEqual(paths,['/v1/tools/sheets_preview_change']);
 }finally{globalThis.fetch=original;f.close();}
});

test('actual registry registers only v2; resolver refuses a tool outside admitted authority',async()=>{
 const f=fixture();try{
  // The host creates the assertion even when admission is absent; plugin lookup
  // cannot turn that unusable closure into owner authority.
  const tool=find(f.resolve(undefined),'sheets_list');
  await assert.rejects(tool.execute('missing',{}),/outside an admitted run or request/);
 }finally{f.close();}
});

test('actual SDK run-scope closure expires retained built tools after completion and abort',async()=>{
 const f=fixture(),original=globalThis.fetch;let sends=0,retained;
 globalThis.fetch=async()=>{sends++;return Response.json({ok:true,data:[]});};
 const abort=new AbortController(),budget=guards.a({signal:abort.signal,abort:error=>abort.abort(error)});
 try{
  await budget.run(async()=>{
   const assertion=guards.r(abort.signal);retained=find(f.resolve(assertion),'sheets_list');
   assert.equal((await retained.execute('active',{})).details.ok,true);
  });
  assert(retained);await assert.rejects(retained.execute('retained',{}),/scope is no longer active/);assert.equal(sends,1);
  const next=new AbortController(),nextBudget=guards.a({signal:next.signal,abort:error=>next.abort(error)});
  await nextBudget.run(async()=>{
   const tool=find(f.resolve(guards.r(next.signal)),'sheets_list');next.abort(Error('synthetic abort'));
   await assert.rejects(tool.execute('cancelled',{}),/synthetic abort/);
  });
  assert.equal(sends,1);
 }finally{globalThis.fetch=original;f.close();}
});

test('actual SDK issued closure blocks retained approval on reset before any Core decision',async()=>{
 const f=fixture(),original=globalThis.fetch,signal=new AbortController();let incarnation='incarnation';const paths=[];
 globalThis.fetch=async(input,init)=>{
  const request=new Request(input,init),path=new URL(request.url).pathname;paths.push(path);
  return Response.json({ok:true,data:{...await request.json(),resource_kind:'sheets_grid',actor:'owner',preview_digest:'synthetic-digest',expires_at:'2099-01-01T00:00:00Z',state:'previewed'}});
 };
 try{
  const tools=f.resolve(()=>{if(incarnation!=='incarnation')throw Error('INVOCATION_EXPIRED');});
  const change={operation_id:'op',spreadsheet:'sheet',view:'grid',action:'update',fields:{range:'A1:A1',values:[['new']]}};
  assert.equal((await find(tools,'sheets_preview_change').execute('preview',change)).details.ok,true);
  const hook=f.builder.registry.typedHooks.find(hook=>hook.hookName==='before_tool_call');assert(hook);
  const approval=await hook.handler({toolName:'sheets_update',params:{operation_id:'op'}},{...f.context,toolName:'sheets_update',abortSignal:signal.signal,requester:{senderId:'owner',senderIsOwner:true}});
  assert(approval.requireApproval);incarnation='new';
  await assert.rejects(approval.requireApproval.onResolution('allow-once'),/INVOCATION_EXPIRED/);
  assert.deepEqual(paths,['/v1/tools/sheets_preview_change']);
 }finally{globalThis.fetch=original;f.close();}
});

test('actual SDK rechecks source authority after an await; removed plugin cannot disclose a pending result',async()=>{
 const f=fixture(),original=globalThis.fetch,pending=deferred(),started=deferred();let sends=0;
 globalThis.fetch=async()=>{sends++;started.resolve();return pending.promise;};
 try{
  const tool=find(f.resolve(()=>{}),'sheets_list'),result=tool.execute('pending',{});
  await started.promise;f.close();pending.resolve(Response.json({ok:true,data:['private fixture']}));
  await assert.rejects(result,/runtime is no longer active/);assert.equal(sends,1);
 }finally{globalThis.fetch=original;f.close();}
});

test('SDK issued final guard prevents a Core dispatch after encoding revokes source authority',async()=>{
 const f=fixture(),original=globalThis.fetch;let active=true,sends=0;
 globalThis.fetch=async()=>{sends++;return Response.json({ok:true,data:{}});};
 try{
  const tool=find(f.resolve(()=>{if(!active)throw Error('INVOCATION_EXPIRED');}),'sheets_preview_change');
  const fields={toJSON(){active=false;return {range:'A1:A1',values:[['new']]};}};
  await assert.rejects(tool.execute('encoding',{operation_id:'op',spreadsheet:'sheet',view:'grid',action:'update',fields}),/INVOCATION_EXPIRED/);
  assert.equal(sends,0);
 }finally{globalThis.fetch=original;f.close();}
});

test('a dispatched Core request is never retried when native cancellation ends its invocation',async()=>{
 const f=fixture(),original=globalThis.fetch,pending=deferred(),started=deferred(),signal=new AbortController();let sends=0;
 globalThis.fetch=async()=>{sends++;started.resolve();return pending.promise;};
 try{
  const tool=find(f.resolve(()=>signal.signal.throwIfAborted()),'sheets_list');
  const response=tool.execute('dispatched',{});await started.promise;signal.abort(Error('synthetic cancellation'));
  pending.resolve(Response.json({ok:true,data:[]}));await assert.rejects(response,/synthetic cancellation/);
  await assert.rejects(tool.execute('no-next-send',{}),/synthetic cancellation/);assert.equal(sends,1);
 }finally{globalThis.fetch=original;f.close();}
});


test('actual v2 issuer binds model task approval and final one-member confirmation to its run',async()=>{
 const f=fixture({taskToolsEnabled:true,taskTransportQualified:true}),original=globalThis.fetch,paths=[];
 const id='33333333-3333-4333-8333-333333333333',digest='a'.repeat(64),nonce='44444444-4444-4444-8444-444444444444';
 const scope={objective:'Correct note',mapping_version:'v1',max_operations:4,targets:[{kind:'document',document:'notes',document_id:'doc',tab_id:'t.0'}]};let phase='proposed',revision=0,retained;
 globalThis.fetch=async(input,init)=>{
  const request=new Request(input,init),path=new URL(request.url).pathname,body=await request.json();paths.push(path);
  assert.equal(request.headers.get('authorization'),'Bearer synthetic-approval');
  if(path.endsWith('/native')){
   if(body.action==='decide'){phase='active';revision++;}
   return Response.json({ok:true,owner:'owner',data:{task_id:id,scope,scope_digest:digest,grant_revision:revision,phase}});
  }
  if(path.endsWith('/execute'))return new Response(JSON.stringify({type:'ready',owner:'owner',challenge:{member:body,nonce,preview_digest:digest}})+'\n'+JSON.stringify({type:'result',owner:'owner',ok:true,data:{state:'applied',operation_id:'op',preview_digest:digest}})+'\n',{headers:{'content-type':'application/vnd.ciel.task-dispatch+ndjson;version=1'}});
  assert(path.endsWith('/confirm'));return Response.json({ok:true,owner:'owner',data:{confirmed:true}});
 };
 const abort=new AbortController(),budget=guards.a({signal:abort.signal,abort:error=>abort.abort(error)});
 try{
  await budget.run(async()=>{
   const tools=f.resolve(guards.r(abort.signal)),proposal=find(tools,'tasks_propose');retained=find(tools,'tasks_execute');
   const hook=f.builder.registry.typedHooks.find(hook=>hook.hookName==='before_tool_call');assert(hook);
   const request={...f.context,abortSignal:abort.signal,requester:{senderId:'owner',senderIsOwner:true}};
   const decision=await hook.handler({toolName:'tasks_propose',params:{scope}},request);assert(decision.requireApproval);
   await decision.requireApproval.onResolution('allow-once');const grant=await proposal.execute('scope',{scope},abort.signal);assert.equal(grant.details.data.grant_revision,1);
   const result=await retained.execute('member',{task_id:id,grant_revision:1,operation_id:'op'},abort.signal);assert.equal(result.details.data.state,'applied');
   assert.equal(paths.filter(path=>path.endsWith('/confirm')).length,1);
  });
  assert(retained);await assert.rejects(retained.execute('retained',{task_id:id,grant_revision:1,operation_id:'op'},abort.signal),/scope is no longer active/);
  assert.equal(paths.filter(path=>path.endsWith('/execute')).length,1);
 }finally{globalThis.fetch=original;f.close();}
});
