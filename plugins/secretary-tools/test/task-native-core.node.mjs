// Actual SDK v2 issuer/broker + built client + real HTTP/Core/bus/SQLite.
// The WorkspacePort is a stateful test source, not a Google HTTP/live qualification.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,dirname} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {startTaskCoreFixture,taskSdkRoot} from './helpers/task-core-fixture.mjs';

for(const profile of ['trusted-client','model-tools'])test(profile+': one actual native task decision drives incremental Core members and stops on uncertainty',{timeout:240000},async()=>{
 const state=await mkdtemp(join(tmpdir(),'ciel-sdk-task-core-'));const saved=new Map(['OPENCLAW_STATE_DIR','OPENCLAW_CONFIG_PATH','SECRETARY_CORE_TOKEN','SECRETARY_APPROVAL_TOKEN'].map(key=>[key,process.env[key]]));
 process.env.OPENCLAW_STATE_DIR=state;process.env.OPENCLAW_CONFIG_PATH=join(state,'config.json');await writeFile(process.env.OPENCLAW_CONFIG_PATH,JSON.stringify({plugins:{enabled:false},logging:{level:'silent'}}));
 process.env.SECRETARY_CORE_TOKEN='synthetic-core-token';process.env.SECRETARY_APPROVAL_TOKEN='synthetic-native-token';
 let core,registry,broker,lifecycle,approval,embedded;
 try{
  core=await startTaskCoreFixture();
  const root=await taskSdkRoot(),load=name=>import(pathToFileURL(join(root,'dist',name)).href);
  const registrySdk=await load('registry-BwDJiK-z.mjs'),toolsSdk=await load('tools-fou-nJCY.mjs'),contextSdk=await load('load-context.resolve-COA_cf7U.mjs'),guards=await load('agent-tool-source-execution-guard-DI8a3ZFX.mjs');
  lifecycle=await load('registry-lifecycle-C978THT2.mjs');approval=await load('agent-tools.before-tool-call-CReBJLEA.mjs');embedded=await load('embedded-mode--Mk71zY-.mjs');
  const {default:entry,createNativeTaskClient}=await import('../dist/index.js');assert.equal(typeof createNativeTaskClient,'function');
  const manifest=JSON.parse(await readFile(new URL('../openclaw.plugin.json',import.meta.url),'utf8'));
  const config={plugins:{enabled:true,allow:['secretary-tools'],entries:{'secretary-tools':{enabled:true}}}},logger={info(){},warn(){},error(message){throw Error(message);},debug(){}};
  const builder=registrySdk.t({logger,runtime:{config:{current:()=>config},gateway:{isAvailable:()=>false}},coreGatewayMethodNames:[],activateGlobalSideEffects:false});registry=builder.registry;
  const source=fileURLToPath(new URL('../dist/index.js',import.meta.url)),record={...manifest,source,rootDir:dirname(dirname(source)),origin:'workspace',format:'bundle',enabled:true,status:'loaded',toolNames:[],hookNames:[],commandNames:[],gatewayMethods:[],cliRegistrars:[],serviceIds:[]};registry.plugins.push(record);
  entry.register(builder.createApi(record,{config,pluginConfig:{owner:'owner',coreUrl:core.url,taskToolsEnabled:true,taskTransportQualified:true,approvalTransportQualified:true},registrationMode:'full'}));assert(registry.tools.every(tool=>tool.contextVersion===2));
  let native;for(const registration of registry.tools){const factory=registration.factory;registration.factory=ctx=>{if(registration.names.includes('docs_apply'))native=ctx;return factory(ctx);};}
  const plugin={...record,toolMetadata:{}},metadataSnapshot={plugins:[plugin],byPluginId:new Map([[plugin.id,plugin]]),index:{plugins:[{pluginId:plugin.id,origin:plugin.origin,enabled:true}]}},loadContext=contextSdk.t({config,env:process.env,workspaceDir:state,logger,manifestRegistry:{plugins:[plugin],diagnostics:[]}});
  const abort=new AbortController(),budget=guards.a({signal:abort.signal,abort:error=>abort.abort(error)});let client;
  let prompts=0;broker=new approval.q();embedded.n(true);approval.Y(broker);broker.subscribe(event=>{if(event.event==='plugin.approval.requested'){prompts++;queueMicrotask(()=>broker.resolve(event.payload.id,'allow-once'));}});
  await budget.run(async()=>{
   const resolved=toolsSdk.i({context:{config,agentId:'agent',sessionKey:'agent:owner',sessionId:'incarnation',requesterSenderId:'owner',senderIsOwner:true,workspaceDir:state},env:process.env,toolAllowlist:['secretary-tools'],assertInvocationCurrent:guards.r(abort.signal),preparedRuntime:{loadContext,metadataSnapshot,registry}});assert(native);
   client=createNativeTaskClient({coreUrl:core.url,owner:'owner',nativeToken:'synthetic-native-token',summaryMemoryEnabled:false,qualified:true},native,abort.signal);
   const scope={objective:'Correct the sample note within its registered tab',mapping_version:'v1',max_operations:10,targets:[{kind:'document',document:'notes',document_id:'doc',tab_id:'t.0'}]};
   let grant;
   if(profile==='trusted-client'){
    const proposal=await client.propose(scope),nativeDecision=await approval.K({deferredApproval:{approval:{...proposal.approval,pluginId:'secretary-tools'},toolName:'docs_apply',toolCallId:'synthetic-task-grant',baseParams:{}},signal:abort.signal});assert.equal(nativeDecision.blocked,false);
    grant=await proposal.grant();
   }else{
    const hook=registry.typedHooks.find(hook=>hook.hookName==='before_tool_call');assert(hook);
    const request={agentId:'agent',sessionKey:'agent:owner',sessionId:'incarnation',abortSignal:abort.signal,requester:{senderId:'owner',senderIsOwner:true}};
    async function run(name,input){
     const tool=resolved.find(tool=>tool.name===name);assert(tool);
     const decision=await hook.handler({toolName:name,params:input},request);assert.notEqual(decision?.block,true);
     if(decision?.requireApproval){const result=await approval.K({deferredApproval:{approval:{...decision.requireApproval,pluginId:'secretary-tools'},toolName:name,toolCallId:'model-grant',baseParams:input},signal:abort.signal});assert.equal(result.blocked,false);}
     const result=await tool.execute('model-call',input,abort.signal);assert.deepEqual(JSON.parse(result.content[0].text),result.details);return result.details;
    }
    const granted=await run('tasks_propose',{scope});assert.equal(granted.ok,true);grant={taskId:granted.data.task_id,revision:granted.data.grant_revision};
    client={capture:async(task_id,change)=>{const result=await run('tasks_capture',{task_id,change});assert.equal(result.ok,true);return result.data;},discard:async(task_id,operation_id)=>{const result=await run('tasks_discard',{task_id,operation_id});assert.equal(result.ok,true);return result.data;},execute:async(task_id,grant_revision,operation_id)=>run('tasks_execute',{task_id,grant_revision,operation_id})};
    client.inspect=async()=>run('tasks_inspect',{task_id:grant.taskId});
    client.end=async(grant_revision)=>run('tasks_end',{task_id:grant.taskId,grant_revision,reason:'cancelled'});
   }
   await client.capture(grant.taskId,{kind:'document',operation_id:'sdk-discarded',document:'notes',tab_id:'t.0',action:'replace_text',fields:{find:'old',replacement:'discarded'}});
   assert.equal((await client.discard(grant.taskId,'sdk-discarded')).state,'rejected');
   for(const [index,find,replacement] of [[1,'old','new'],[2,'new','second'],[3,'second','final']]){
    const operation_id='sdk-task-'+index;if(index===3)await core.fixture('uncertain');
    const change={kind:'document',operation_id,document:'notes',tab_id:'t.0',action:index===1?'replace_text':'replace_text_batch',fields:index===1?{find,replacement}:{replacements:[{find,replacement},{find:index===2?'note':'record',replacement:index===2?'record':'memo'}]}};
    const preview=await client.capture(grant.taskId,change);assert.equal(preview.state,'previewed');
    if(index===2)assert.equal(preview.after.text,'second record\n');
    assert.equal((await client.capture(grant.taskId,change)).preview_digest,preview.preview_digest);
    const result=await client.execute(grant.taskId,grant.revision,operation_id);assert.equal(result.data.state,index===3?'uncertain':'applied');
    const replay=await client.execute(grant.taskId,grant.revision,operation_id);assert.equal(replay.ok,false);assert.equal((await core.fixture('counts')).sends,index);
   }
   if(profile==='model-tools'){
    const observed=await client.inspect();assert.equal(observed.ok,true);assert.equal(observed.data.phase,'suspended');
    assert.equal((await client.end(observed.data.grant_revision)).data.phase,'closed');
   }
   assert.equal(prompts,1);
  });
  await assert.rejects(client.capture('retained',{}),/scope is no longer active/);assert.equal((await core.fixture('counts')).sends,3);
 }finally{
  if(broker){broker.stop();approval.J(broker);embedded.n(false);}if(registry)lifecycle.T(registry);if(core)await core.close();
  for(const [key,value] of saved){if(value===undefined)delete process.env[key];else process.env[key]=value;}await rm(state,{recursive:true,force:true});
 }
});
