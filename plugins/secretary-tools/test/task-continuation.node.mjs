// Actual SDK v2 run guards/lifecycle emitters and real Core HTTP/journal.
// Requester, session transitions and WorkspacePort are fixture inputs, not live routing.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,dirname} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {startTaskCoreFixture,taskSdkRoot} from './helpers/task-core-fixture.mjs';
const scope={objective:'Correct the sample note',mapping_version:'v1',max_operations:8,targets:[{kind:'document',document:'notes',document_id:'doc',tab_id:'t.0'}]};
const change=(operation_id,find,replacement)=>({kind:'document',operation_id,document:'notes',tab_id:'t.0',action:'replace_text',fields:{find,replacement}});
async function fixture(){
 const originalFetch=globalThis.fetch,nativeActions=[];
 const state=await mkdtemp(join(tmpdir(),'ciel-task-continuation-')),saved=new Map(['OPENCLAW_STATE_DIR','OPENCLAW_CONFIG_PATH','SECRETARY_CORE_TOKEN','SECRETARY_APPROVAL_TOKEN'].map(key=>[key,process.env[key]]));
 process.env.OPENCLAW_STATE_DIR=state;process.env.OPENCLAW_CONFIG_PATH=join(state,'config.json');process.env.SECRETARY_CORE_TOKEN='synthetic-core-token';process.env.SECRETARY_APPROVAL_TOKEN='synthetic-native-token';
 await writeFile(process.env.OPENCLAW_CONFIG_PATH,JSON.stringify({plugins:{enabled:false},logging:{level:'silent'}}));
 let core;try{
 core=await startTaskCoreFixture();
 globalThis.fetch=(input,init)=>{const url=new URL(input instanceof Request?input.url:String(input));if(url.origin===core.url&&url.pathname==='/v1/write-tasks/native'&&typeof init?.body==='string')nativeActions.push(JSON.parse(init.body).action);return originalFetch(input,init);};
 const root=await taskSdkRoot(),load=name=>import(pathToFileURL(join(root,'dist',name)).href);
 const registrySdk=await load('registry-BwDJiK-z.mjs'),toolsSdk=await load('tools-fou-nJCY.mjs'),contextSdk=await load('load-context.resolve-COA_cf7U.mjs'),guards=await load('agent-tool-source-execution-guard-DI8a3ZFX.mjs'),lifecycle=await load('registry-lifecycle-C978THT2.mjs'),approval=await load('agent-tools.before-tool-call-CReBJLEA.mjs'),embedded=await load('embedded-mode--Mk71zY-.mjs'),globalHooks=await load('hook-runner-global-trQbJiN5.mjs'),resetService=await load('session-reset-service-DN3eiupo.mjs');
 const {default:entry}=await import('../dist/index.js'),manifest=JSON.parse(await readFile(new URL('../openclaw.plugin.json',import.meta.url),'utf8'));
 const config={plugins:{enabled:true,allow:['secretary-tools'],entries:{'secretary-tools':{enabled:true}}}},logger={info(){},warn(){},error(message){throw Error(message);},debug(){}};
 const builder=registrySdk.t({logger,runtime:{config:{current:()=>config},gateway:{isAvailable:()=>false}},coreGatewayMethodNames:[],activateGlobalSideEffects:false});
 const source=fileURLToPath(new URL('../dist/index.js',import.meta.url)),record={...manifest,source,rootDir:dirname(dirname(source)),origin:'workspace',format:'bundle',enabled:true,status:'loaded',toolNames:[],hookNames:[],commandNames:[],gatewayMethods:[],cliRegistrars:[],serviceIds:[]};builder.registry.plugins.push(record);
 const api=builder.createApi(record,{config,pluginConfig:{owner:'owner',coreUrl:core.url,taskToolsEnabled:true,taskTransportQualified:true,approvalTransportQualified:true},registrationMode:'full'});entry.register(api);
 const plugin={...record,toolMetadata:{}},metadataSnapshot={plugins:[plugin],byPluginId:new Map([[plugin.id,plugin]]),index:{plugins:[{pluginId:plugin.id,origin:plugin.origin,enabled:true}]}},loadContext=contextSdk.t({config,env:process.env,workspaceDir:state,logger,manifestRegistry:{plugins:[plugin],diagnostics:[]}});
 const broker=new approval.q();let prompts=0;embedded.n(true);approval.Y(broker);broker.subscribe(event=>{if(event.event==='plugin.approval.requested'){prompts++;queueMicrotask(()=>broker.resolve(event.payload.id,'allow-once'));}});
 const identity={agentId:'agent',sessionKey:'agent:owner',sessionId:'incarnation'};
 async function inRun(sessionId,action){
  const abort=new AbortController(),budget=guards.a({signal:abort.signal,abort:error=>abort.abort(error)});
  return budget.run(async()=>{
   const context={config,...identity,sessionId,requesterSenderId:'owner',senderIsOwner:true,workspaceDir:state};
   const tools=toolsSdk.i({context,env:process.env,toolAllowlist:['secretary-tools'],assertInvocationCurrent:guards.r(abort.signal),preparedRuntime:{loadContext,metadataSnapshot,registry:builder.registry}});
   const hook=builder.registry.typedHooks.find(hook=>hook.hookName==='before_tool_call');assert(hook);
   const request={...context,abortSignal:abort.signal,requester:{senderId:'owner',senderIsOwner:true}};
   async function run(name,input){
    const tool=tools.find(tool=>tool.name===name);assert(tool);const decision=await hook.handler({toolName:name,params:input},request);assert.notEqual(decision?.block,true,decision?.blockReason);
    if(decision?.requireApproval){const allowed=await approval.K({deferredApproval:{approval:{...decision.requireApproval,pluginId:'secretary-tools'},toolName:name,toolCallId:'continuation-grant',baseParams:input},signal:abort.signal});assert.equal(allowed.blocked,false);}
    const result=await tool.execute('continuation-member',input,abort.signal);return result.details;
   }
   return action({run,tool:name=>tools.find(tool=>tool.name===name),signal:abort.signal});
  });
 }
 async function end(reason){
  const observed=new Promise(resolve=>api.on('session_end',resolve));globalHooks.a(builder.registry);
  if(reason==='reset')await resetService.n({cfg:config,target:{agentId:identity.agentId,canonicalKey:identity.sessionKey},entry:{sessionId:identity.sessionId},storePath:join(state,'sessions.json'),messages:[],reason:'reset'});
  resetService.a({cfg:config,...identity,reason,storePath:join(state,'sessions.json'),nextSessionId:'successor',nextSessionKey:identity.sessionKey});
  let timer;try{await Promise.race([observed,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Session-end emitter did not deliver')),10000);})]);}finally{clearTimeout(timer);}
 }
 return {core,api,inRun,end,prompts:()=>prompts,invalidations:()=>nativeActions.filter(action=>action==='invalidate').length,async close(){broker.stop();approval.J(broker);embedded.n(false);globalHooks.o();lifecycle.T(builder.registry);await core.close();for(const [key,value] of saved){if(value===undefined)delete process.env[key];else process.env[key]=value;}globalThis.fetch=originalFetch;await rm(state,{recursive:true,force:true});}};
 }catch(error){if(core)await core.close();globalThis.fetch=originalFetch;for(const [key,value] of saved){if(value===undefined)delete process.env[key];else process.env[key]=value;}await rm(state,{recursive:true,force:true});throw error;}
}
test('a new admitted owner turn continues the same task without renewing its scope decision',{timeout:240000},async()=>{
 const f=await fixture();try{
  let grant,retained;
  await f.inRun('incarnation',async({run,tool})=>{
   grant=(await run('tasks_propose',{scope})).data;retained=tool('tasks_execute');
   const operation_id='turn-one';assert.equal((await run('tasks_capture',{task_id:grant.task_id,change:change(operation_id,'old','first')})).data.state,'previewed');
   assert.equal((await run('tasks_execute',{task_id:grant.task_id,grant_revision:grant.grant_revision,operation_id})).data.state,'applied');
  });
  assert(retained);await assert.rejects(retained.execute('retained',{task_id:grant.task_id,grant_revision:grant.grant_revision,operation_id:'turn-one'},new AbortController().signal),/scope is no longer active/);
  await f.inRun('incarnation',async({run})=>{
   const observed=await run('tasks_inspect',{task_id:grant.task_id});assert.equal(observed.data.phase,'active');assert.equal(observed.data.admitted_operations,1);
   const operation_id='turn-two';await run('tasks_capture',{task_id:grant.task_id,change:change(operation_id,'first','second')});assert.equal((await run('tasks_execute',{task_id:grant.task_id,grant_revision:grant.grant_revision,operation_id})).data.state,'applied');
   assert.equal((await run('tasks_inspect',{task_id:grant.task_id})).data.admitted_operations,2);await run('tasks_end',{task_id:grant.task_id,grant_revision:grant.grant_revision,reason:'completed'});
  });
  assert.equal(f.prompts(),1);assert.equal((await f.core.fixture('counts')).sends,2);
 }finally{await f.close();}
});
for(const reason of ['new','reset'])test('actual '+reason+' emitter retires the old task and permits a separately approved successor',{timeout:240000},async()=>{
 const f=await fixture();try{
  let old;
  await f.inRun('incarnation',async({run})=>{old=(await run('tasks_propose',{scope})).data;await run('tasks_capture',{task_id:old.task_id,change:change('unsent-old','old','discarded')});});
  await f.end(reason);
  await f.inRun('successor',async({run})=>{
   const response=await run('tasks_propose',{scope});assert.equal(response.ok,true);const next=response.data;assert.notEqual(next.task_id,old.task_id);
   const history=await run('operations_get_history',{operation_id:'unsent-old'});assert.equal(history.data.operation.state,'rejected');
   await f.end(reason);assert.equal((await run('tasks_inspect',{task_id:next.task_id})).data.phase,'active');
   const operation_id='successor-one';await run('tasks_capture',{task_id:next.task_id,change:change(operation_id,'old','final')});assert.equal((await run('tasks_execute',{task_id:next.task_id,grant_revision:next.grant_revision,operation_id})).data.state,'applied');
   await run('tasks_end',{task_id:next.task_id,grant_revision:next.grant_revision,reason:'completed'});
  });
  assert.equal(f.prompts(),2);assert.equal(f.invalidations(),1);assert.equal((await f.core.fixture('counts')).sends,1);
 }finally{await f.close();}
});

test('reset retains an uncertain member and a new scope cannot bypass its source fence',{timeout:240000},async()=>{
 const f=await fixture();try{
  await f.inRun('incarnation',async({run})=>{
   const grant=(await run('tasks_propose',{scope})).data,operation_id='uncertain-old';await run('tasks_capture',{task_id:grant.task_id,change:change(operation_id,'old','accepted')});await f.core.fixture('uncertain');
   assert.equal((await run('tasks_execute',{task_id:grant.task_id,grant_revision:grant.grant_revision,operation_id})).data.state,'uncertain');
  });
  await f.end('reset');
  await f.inRun('successor',async({run})=>{
   const response=await run('tasks_propose',{scope});assert.equal(response.ok,true);
   const capture=await run('tasks_capture',{task_id:response.data.task_id,change:change('forbidden-replacement','accepted','new')});assert.equal(capture.ok,false);assert.equal(capture.error.code,'TASK_CORE_REJECTED');
   assert.equal((await run('operations_get_history',{operation_id:'uncertain-old'})).data.operation.state,'uncertain');
   await run('tasks_end',{task_id:response.data.task_id,grant_revision:response.data.grant_revision,reason:'cancelled'});
  });
  assert.equal(f.prompts(),2);assert.equal(f.invalidations(),1);assert.equal((await f.core.fixture('counts')).sends,1);
 }finally{await f.close();}
});
