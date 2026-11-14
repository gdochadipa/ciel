// Actual SDK v2 issuer/broker + built client + real HTTP/Core/bus/SQLite.
// The WorkspacePort is a stateful test source, not a Google HTTP/live qualification.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,dirname} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const project=fileURLToPath(new URL('../../../',import.meta.url));
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
async function server(){
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
async function sdkRoot(){let path=dirname(fileURLToPath(import.meta.resolve('openclaw/plugin-sdk/plugin-entry')));for(let depth=0;depth<5;depth++,path=dirname(path)){try{const pkg=JSON.parse(await readFile(join(path,'package.json'),'utf8'));if(pkg.name==='openclaw'){assert.equal(pkg.version,'2026.10.7');return path;}}catch(error){if(error?.code!=='ENOENT')throw error;}}throw Error('Pinned SDK not found');}

test('one actual native task decision drives incremental Core members and stops on uncertainty',{timeout:240000},async()=>{
 const state=await mkdtemp(join(tmpdir(),'ciel-sdk-task-core-'));const saved=new Map(['OPENCLAW_STATE_DIR','OPENCLAW_CONFIG_PATH'].map(key=>[key,process.env[key]]));
 process.env.OPENCLAW_STATE_DIR=state;process.env.OPENCLAW_CONFIG_PATH=join(state,'config.json');await writeFile(process.env.OPENCLAW_CONFIG_PATH,JSON.stringify({plugins:{enabled:false},logging:{level:'silent'}}));
 let core,registry,broker,lifecycle,approval,embedded;
 try{
  core=await server();
  const root=await sdkRoot(),load=name=>import(pathToFileURL(join(root,'dist',name)).href);
  const registrySdk=await load('registry-BwDJiK-z.mjs'),toolsSdk=await load('tools-fou-nJCY.mjs'),contextSdk=await load('load-context.resolve-COA_cf7U.mjs'),guards=await load('agent-tool-source-execution-guard-DI8a3ZFX.mjs');
  lifecycle=await load('registry-lifecycle-C978THT2.mjs');approval=await load('agent-tools.before-tool-call-CReBJLEA.mjs');embedded=await load('embedded-mode--Mk71zY-.mjs');
  const {default:entry,createNativeTaskClient}=await import('../dist/index.js');assert.equal(typeof createNativeTaskClient,'function');
  const manifest=JSON.parse(await readFile(new URL('../openclaw.plugin.json',import.meta.url),'utf8'));
  const config={plugins:{enabled:true,allow:['secretary-tools'],entries:{'secretary-tools':{enabled:true}}}},logger={info(){},warn(){},error(message){throw Error(message);},debug(){}};
  const builder=registrySdk.t({logger,runtime:{config:{current:()=>config},gateway:{isAvailable:()=>false}},coreGatewayMethodNames:[],activateGlobalSideEffects:false});registry=builder.registry;
  const source=fileURLToPath(new URL('../dist/index.js',import.meta.url)),record={...manifest,source,rootDir:dirname(dirname(source)),origin:'workspace',format:'bundle',enabled:true,status:'loaded',toolNames:[],hookNames:[],commandNames:[],gatewayMethods:[],cliRegistrars:[],serviceIds:[]};registry.plugins.push(record);
  entry.register(builder.createApi(record,{config,pluginConfig:{owner:'owner'},registrationMode:'full'}));assert(registry.tools.every(tool=>tool.contextVersion===2));
  let native;for(const registration of registry.tools){const factory=registration.factory;registration.factory=ctx=>{if(registration.names.includes('docs_apply'))native=ctx;return factory(ctx);};}
  const plugin={...record,toolMetadata:{}},metadataSnapshot={plugins:[plugin],byPluginId:new Map([[plugin.id,plugin]]),index:{plugins:[{pluginId:plugin.id,origin:plugin.origin,enabled:true}]}},loadContext=contextSdk.t({config,env:process.env,workspaceDir:state,logger,manifestRegistry:{plugins:[plugin],diagnostics:[]}});
  const abort=new AbortController(),budget=guards.a({signal:abort.signal,abort:error=>abort.abort(error)});let client;
  let prompts=0;broker=new approval.q();embedded.n(true);approval.Y(broker);broker.subscribe(event=>{if(event.event==='plugin.approval.requested'){prompts++;queueMicrotask(()=>broker.resolve(event.payload.id,'allow-once'));}});
  await budget.run(async()=>{
   toolsSdk.i({context:{config,agentId:'agent',sessionKey:'agent:owner',sessionId:'incarnation',requesterSenderId:'owner',senderIsOwner:true,workspaceDir:state},env:process.env,toolAllowlist:['secretary-tools'],assertInvocationCurrent:guards.r(abort.signal),preparedRuntime:{loadContext,metadataSnapshot,registry}});assert(native);
   client=createNativeTaskClient({coreUrl:core.url,owner:'owner',nativeToken:'synthetic-native-token',summaryMemoryEnabled:false,qualified:true},native,abort.signal);
   const scope={objective:'Correct the sample note within its registered tab',mapping_version:'v1',max_operations:10,targets:[{kind:'document',document:'notes',document_id:'doc',tab_id:'t.0'}]};
   const proposal=await client.propose(scope),nativeDecision=await approval.K({deferredApproval:{approval:{...proposal.approval,pluginId:'secretary-tools'},toolName:'docs_apply',toolCallId:'synthetic-task-grant',baseParams:{}},signal:abort.signal});assert.equal(nativeDecision.blocked,false);
   const grant=await proposal.grant();
   await client.capture(grant.taskId,{kind:'document',operation_id:'sdk-discarded',document:'notes',tab_id:'t.0',action:'replace_text',fields:{find:'old',replacement:'discarded'}});
   assert.equal((await client.discard(grant.taskId,'sdk-discarded')).state,'rejected');
   for(const [index,find,replacement] of [[1,'old','new'],[2,'new','second'],[3,'second','final']]){
    const operation_id='sdk-task-'+index;if(index===3)await core.fixture('uncertain');
    const preview=await client.capture(grant.taskId,{kind:'document',operation_id,document:'notes',tab_id:'t.0',action:'replace_text',fields:{find,replacement}});assert.equal(preview.state,'previewed');
    const result=await client.execute(grant.taskId,grant.revision,operation_id);assert.equal(result.data.state,index===3?'uncertain':'applied');
    const replay=await client.execute(grant.taskId,grant.revision,operation_id);assert.equal(replay.ok,false);assert.equal((await core.fixture('counts')).sends,index);
   }
   assert.equal(prompts,1);
  });
  await assert.rejects(client.capture('retained',{}),/scope is no longer active/);assert.equal((await core.fixture('counts')).sends,3);
 }finally{
  if(broker){broker.stop();approval.J(broker);embedded.n(false);}if(registry)lifecycle.T(registry);if(core)await core.close();
  for(const [key,value] of saved){if(value===undefined)delete process.env[key];else process.env[key]=value;}await rm(state,{recursive:true,force:true});
 }
});
