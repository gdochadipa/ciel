// Actual pinned approval broker + built model tools, synthetic Core/host inputs.
// The companion task-native-core suite covers real v2 issuance and HTTP/journal.
import {test,after} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,dirname} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const state=await mkdtemp(join(tmpdir(),'ciel-task-tools-'));
const saved=new Map(['OPENCLAW_STATE_DIR','OPENCLAW_CONFIG_PATH','SECRETARY_CORE_TOKEN','SECRETARY_APPROVAL_TOKEN'].map(key=>[key,process.env[key]]));
process.env.OPENCLAW_STATE_DIR=state;process.env.OPENCLAW_CONFIG_PATH=join(state,'config.json');
process.env.SECRETARY_CORE_TOKEN='synthetic-core-token';process.env.SECRETARY_APPROVAL_TOKEN='synthetic-native-token';
await writeFile(process.env.OPENCLAW_CONFIG_PATH,JSON.stringify({plugins:{enabled:false},logging:{level:'silent'}}));
after(async()=>{for(const [key,value] of saved){if(value===undefined)delete process.env[key];else process.env[key]=value;}await rm(state,{recursive:true,force:true});});
let root=dirname(fileURLToPath(import.meta.resolve('openclaw/plugin-sdk/plugin-entry')));
for(let depth=0;depth<5;depth++,root=dirname(root)){
 try{const pkg=JSON.parse(await readFile(join(root,'package.json'),'utf8'));if(pkg.name==='openclaw'){assert.equal(pkg.version,'2026.10.7');break;}}catch(error){if(error?.code!=='ENOENT')throw error;}
}
const sdk=await import(pathToFileURL(join(root,'dist/agent-tools.before-tool-call-CReBJLEA.mjs')).href),embedded=await import(pathToFileURL(join(root,'dist/embedded-mode--Mk71zY-.mjs')).href);
const {default:entry}=await import('../dist/index.js');
const digest='a'.repeat(64),id='11111111-1111-4111-8111-111111111111',nonce='22222222-2222-4222-8222-222222222222';
const scope={objective:'Correct note',mapping_version:'v1',max_operations:4,targets:[{kind:'document',document:'notes',document_id:'doc',tab_id:'t.0'}]};
let nextPort=21000;
function deferred(){let resolve;const promise=new Promise(done=>{resolve=done;});return {promise,resolve};}
function fixture(settings={}){
 const original=globalThis.fetch,ack=deferred(),started=deferred(),paths=[],bodies=[],hooks=new Map(),factories=new Map(),abort=new AbortController();let active=true,revision=0,phase='proposed',prompts=0,held=false,refused=false,reset=false;
 const api={pluginConfig:{owner:'owner',coreUrl:'http://127.0.0.1:'+nextPort++,taskToolsEnabled:true,taskTransportQualified:true,approvalTransportQualified:true,...settings},on(name,handler){hooks.set(name,handler);},registerTool(factory,options){factories.set(options.name,factory);},get runtime(){throw Error('Unexpected native runtime access');}};
 entry.register(api);
 const context={agentId:'agent',sessionKey:'agent:owner',sessionId:'incarnation',requesterSenderId:'owner',senderIsOwner:true,assertInvocationCurrent(){if(!active)throw Error('INVOCATION_EXPIRED');}},tools=new Map([...factories].map(([name,factory])=>[name,factory.create(context)]));
 const request={...context,abortSignal:abort.signal,requester:{senderId:'owner',senderIsOwner:true}};
 globalThis.fetch=async(input,init)=>{
  const request=new Request(input,init),path=new URL(request.url).pathname,body=await request.json();assert.equal(request.headers.get('authorization'),'Bearer synthetic-native-token');paths.push(path);bodies.push(body);
  if(path.endsWith('/native')){
   if(body.action==='decide'){started.resolve();if(held)await ack.promise;if(refused)return Response.json({ok:false},{status:409});phase=body.decision==='approve'?'active':'closed';revision++;}
   if(body.action==='capture')return Response.json({ok:true,owner:'owner',data:{operation_id:body.change.operation_id,state:'previewed'}});
   if(body.action==='end')phase='closed';
   return Response.json({ok:true,owner:'owner',data:{task_id:id,scope,scope_digest:digest,grant_revision:revision,phase,requires_confirmation:phase==='suspended'}});
  }
  if(path.endsWith('/execute')){
   if(reset)await hooks.get('before_reset')({},context);
   return new Response(JSON.stringify({type:'ready',owner:'owner',challenge:{member:body,nonce,preview_digest:digest}})+'\n'+JSON.stringify({type:'result',owner:'owner',ok:true,data:{operation_id:body.operation_id,state:'applied',preview_digest:digest}})+'\n',{headers:{'content-type':'application/vnd.ciel.task-dispatch+ndjson;version=1'}});
  }
  if(path.endsWith('/confirm'))return Response.json({ok:true,owner:'owner',data:{confirmed:true}});
  throw Error('Unexpected Core route');
 };
 const broker=new sdk.q();embedded.n(true);sdk.Y(broker);broker.subscribe(event=>{if(event.event==='plugin.approval.requested'){prompts++;queueMicrotask(()=>broker.resolve(event.payload.id,'allow-once'));}});
 async function run(name,input,signal=abort.signal){const result=await tools.get(name).execute('model-call',input,signal);if(result.content.length)assert.deepEqual(JSON.parse(result.content[0].text),result.details);return result.details;}
 async function before(name,input,extra={}){return hooks.get('before_tool_call')({toolName:name,params:input},{...request,...extra});}
 async function approve(name,input){const decision=await before(name,input);assert(decision.requireApproval);const result=await sdk.K({deferredApproval:{approval:{...decision.requireApproval,pluginId:'secretary-tools'},toolName:name,toolCallId:'native-prompt',baseParams:input},signal:abort.signal});assert.equal(result.blocked,false);return decision.requireApproval;}
 return {api,run,before,approve,paths,bodies,started,ack,prompts:()=>prompts,hold(){held=true;},refuse(){refused=true;},resetDuringPreflight(){reset=true;},revoke(){active=false;},suspend(){phase='suspended';revision=3;},close(){broker.stop();sdk.J(broker);embedded.n(false);globalThis.fetch=original;}};
}
test('built model proposal awaits the actual SDK detached callback ACK, then members need no new prompt',async()=>{
 const f=fixture();try{
  f.hold();await f.approve('tasks_propose',{scope});await f.started.promise;
  let finished=false;const result=f.run('tasks_propose',{scope}).then(value=>{finished=true;return value;});
  await new Promise(resolve=>setImmediate(resolve));assert.equal(finished,false);assert.equal(f.paths.length,2);
  f.ack.resolve();assert.equal((await result).data.grant_revision,1);
  const change={kind:'document',operation_id:'op',document:'notes',tab_id:'t.0',action:'replace_text',fields:{find:'old',replacement:'new'}};
  assert.equal(await f.before('tasks_capture',{task_id:id,change}),undefined);
  assert.equal((await f.run('tasks_capture',{task_id:id,change})).data.state,'previewed');
  assert.equal((await f.run('tasks_execute',{task_id:id,grant_revision:1,operation_id:'op'})).data.state,'applied');
  assert.equal(f.paths.filter(path=>path.endsWith('/execute')).length,1);assert.equal(f.paths.filter(path=>path.endsWith('/confirm')).length,1);assert.equal(f.prompts(),1);
  assert.equal((await f.run('tasks_inspect',{task_id:id})).data.phase,'active');assert.equal((await f.run('tasks_end',{task_id:id,grant_revision:1,reason:'completed'})).data.phase,'closed');
 }finally{f.close();}
});
test('built task tools default off, reject read-only/unqualified/equal-credential profiles before Core traffic',async()=>{
 for(const [config,code] of [[{taskToolsEnabled:false},'TASK_TOOLS_DISABLED'],[{readOnlyRuntime:true},'READONLY_TASKS_DISABLED'],[{taskTransportQualified:false},'TASK_TRANSPORT_UNQUALIFIED'],[{approvalTransportQualified:false},'TASK_TRANSPORT_UNQUALIFIED']]){
  const f=fixture(config);try{assert.equal((await f.run('tasks_inspect',{task_id:id})).error.code,code);assert.equal((await f.before('tasks_propose',{scope})).block,true);assert.equal(f.paths.length,0);}finally{f.close();}
 }
 const f=fixture();const saved=process.env.SECRETARY_APPROVAL_TOKEN;try{process.env.SECRETARY_APPROVAL_TOKEN=process.env.SECRETARY_CORE_TOKEN;assert.equal((await f.run('tasks_inspect',{task_id:id})).error.code,'INVALID_CONFIGURATION');assert.equal(f.paths.length,0);}finally{process.env.SECRETARY_APPROVAL_TOKEN=saved;f.close();}
});
test('model execution without a hook cannot propose or approve; changed scope cannot borrow a prompt',async()=>{
 const f=fixture();try{
  assert.equal((await f.run('tasks_propose',{scope})).error.code,'TASK_APPROVAL_REQUIRED');assert.equal(f.paths.length,0);
  assert.equal((await f.before('tasks_propose',{scope},{requester:{senderId:'other',senderIsOwner:true}})).block,true);assert.equal(f.paths.length,0);
  await f.approve('tasks_propose',{scope});assert.equal((await f.run('tasks_propose',{scope:{...scope,objective:'changed'}})).error.code,'TASK_APPROVAL_MISMATCH');assert.equal(f.paths.length,2);
  assert.equal((await f.run('tasks_propose',{scope})).data.task_id,id);
 }finally{f.close();}
});
test('a failed detached Core grant acknowledgement returns its task ID and never captures or dispatches',async()=>{
 const f=fixture();try{f.refuse();await f.approve('tasks_propose',{scope});const result=await f.run('tasks_propose',{scope});assert.equal(result.error.code,'TASK_CORE_REJECTED');assert.equal(result.task_id,id);assert.equal(f.paths.length,2);}finally{f.close();}
});
test('delivered reset after source preflight withholds the one-member confirmation',async()=>{
 const f=fixture();try{await f.approve('tasks_propose',{scope});await f.run('tasks_propose',{scope});f.resetDuringPreflight();const result=await f.run('tasks_execute',{task_id:id,grant_revision:1,operation_id:'op'});assert.equal(result.error.code,'INVOCATION_EXPIRED');assert.equal(f.paths.filter(path=>path.endsWith('/confirm')).length,0);assert.equal(f.paths.filter(path=>path.endsWith('/execute')).length,1);}finally{f.close();}
});
test('task reconfirmation uses its retained ID and revision; revoked ACK cannot disclose a grant',async()=>{
 const f=fixture();try{f.suspend();await f.approve('tasks_resume',{task_id:id});const result=await f.run('tasks_resume',{task_id:id});assert.equal(result.data.grant_revision,4);assert.equal(f.bodies[0].action,'review');assert.equal(f.bodies[1].task_id,id);assert.equal(f.bodies[1].grant_revision,3);assert.equal(f.prompts(),1);}finally{f.close();}
 const revoked=fixture();try{revoked.hold();await revoked.approve('tasks_propose',{scope});await revoked.started.promise;const pending=revoked.run('tasks_propose',{scope});revoked.revoke();revoked.ack.resolve();assert.equal((await pending).error.code,'INVOCATION_EXPIRED');assert.equal(revoked.paths.length,2);}finally{revoked.close();}
});
