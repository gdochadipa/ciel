// Built client + actual SDK safe-root memory IO. Core/source/model replies are synthetic.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,rm,readFile,readdir,writeFile,mkdir,symlink} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import entry,{createMemoryNativeTaskClient} from '../dist/index.js';

const id='11111111-1111-4111-8111-111111111111',nonce='22222222-2222-4222-8222-222222222222',digest='a'.repeat(64);
let port=23000;
const scope={objective:'Correct the bounded sample note',mapping_version:'v1',max_operations:10,targets:[{kind:'document',document:'notes',document_id:'doc',tab_id:'t.0'}]};
function deferred(){let resolve;const promise=new Promise(done=>{resolve=done;});return {promise,resolve};}
async function fixture(){
 const workspace=await mkdtemp(join(tmpdir(),'ciel-task-memory-')),original=globalThis.fetch;
 const saved=process.env.SECRETARY_CORE_TOKEN;process.env.SECRETARY_CORE_TOKEN='synthetic-core-token';
 const paths=[],histories=new Map(),tools=new Map(),hooks=new Map(),abort=new AbortController();
 const state={text:'old note\n',sends:0,models:0,reads:0,phase:'proposed',revision:0,outcome:'applied',current:true,audience:true,modelError:false,registryId:'doc',version:'v1',onExecute:undefined,onConfirm:undefined,onDecision:undefined,historyError:false};
 const context={agentId:'agent',sessionKey:'agent:owner',sessionId:'incarnation',workspaceDir:workspace,requesterSenderId:'owner',senderIsOwner:true,sandboxed:false,memoryAudience:{kind:'owner-private',agentId:'agent'},assertInvocationCurrent(){if(!state.current)throw Error('INVOCATION_EXPIRED');},assertMemoryAudienceCurrent(){if(!state.audience)throw Error('MEMORY_AUDIENCE_EXPIRED');}};
 const coreUrl='http://127.0.0.1:'+port++;
 const api={pluginConfig:{owner:'owner',summaryMemoryEnabled:true,coreUrl},config:{plugins:{entries:{'memory-core':{config:{dreaming:{enabled:false}}}}}},runtime:{gateway:{isAvailable:()=>false},llm:{async complete(){state.models++;if(state.modelError)throw Error('Synthetic model failure');return {text:JSON.stringify({summary:'A bounded note [S1].',excerpt_ids:['S1']})};}}},registerTool(factory,options){tools.set(options.name,factory);},on(name,handler){hooks.set(name,handler);}};
 entry.register(api);
 const note=join(workspace,'memory/ciel-docs',createHash('sha256').update('notes\0doc').digest('hex')+'.md');
 let pending;
 function operation(operationId){let op=histories.get(operationId);if(op===undefined){op={operation_id:operationId,resource_kind:'google_docs',document:'notes',record_id:'doc',mapping_version:'v1',state:'previewed'};histories.set(operationId,op);}return op;}
 globalThis.fetch=async(input,init)=>{
  const request=new Request(input,init),url=new URL(request.url),body=await request.json();assert.equal(url.origin,coreUrl);paths.push(url.pathname);
  const native=url.pathname.startsWith('/v1/write-tasks/');assert.equal(request.headers.get('authorization'),'Bearer '+(native?'synthetic-native-token':'synthetic-core-token'));
  if(url.pathname.endsWith('/native')){
   if(body.action==='decide'){await state.onDecision?.();state.phase=body.decision==='approve'?'active':'closed';state.revision++;}
   return Response.json({ok:true,owner:'owner',data:{task_id:id,scope_digest:digest,scope,phase:state.phase,grant_revision:state.revision}});
  }
  if(url.pathname.endsWith('/operations_get_history'))return state.historyError?Response.json({ok:false,error:{code:'not_found'}},{status:404}):Response.json({ok:true,data:{operation:operation(body.operation_id)}});
  if(url.pathname.endsWith('/docs_list'))return Response.json({ok:true,data:{mapping_version:state.version,documents:[{document:'notes',id:state.registryId,readable:true}]}});
  if(url.pathname.endsWith('/docs_read')){
   state.reads++;const n=[...state.text].length;
   return Response.json({ok:true,data:{mode:'summary_context',content_role:'untrusted_source_data',context_id:'b'.repeat(64),source:{document:'notes',document_id:'doc',mapping_version:'v1',title:'Sample',revision_id:'r'+state.sends,source_fingerprint:createHash('sha256').update(state.text).digest('hex'),retrieved_at:new Date().toISOString()},excerpts:[{excerpt_id:'S1',text:state.text,source:{tab_id:'t.0',start_character:0,end_character:n}}],coverage:{scope:'projected_body_text_only',total_characters:n,included_characters:n,omitted_characters:0,total_tabs:1,included_tabs:1,projected_text_complete:true,can_summarize:true}}});
  }
  if(url.pathname.endsWith('/execute')){
   await state.onExecute?.();
   return new Response(new ReadableStream({start(controller){pending={controller,member:body};controller.enqueue(new TextEncoder().encode(JSON.stringify({type:'ready',owner:'owner',challenge:{member:body,preview_digest:digest,nonce}})+'\n'));}}),{headers:{'content-type':'application/vnd.ciel.task-dispatch+ndjson;version=1'}});
  }
  if(url.pathname.endsWith('/confirm')){
   assert(pending);await state.onConfirm?.();state.sends++;state.text='updated '+state.sends+'\n';operation(pending.member.operation_id).state=state.outcome;
   pending.controller.enqueue(new TextEncoder().encode(JSON.stringify({type:'result',owner:'owner',ok:true,data:{state:state.outcome,operation_id:pending.member.operation_id,preview_digest:digest}})+'\n'));pending.controller.close();pending=undefined;
   return Response.json({ok:true,owner:'owner',data:{confirmed:true}});
  }
  throw Error('Unexpected synthetic route: '+url.pathname);
 };
 const options={qualified:true,coreUrl,owner:'owner',nativeToken:'synthetic-native-token',coreToken:'synthetic-core-token',summaryMemoryEnabled:true};
 const client=()=>createMemoryNativeTaskClient(options,api,context,abort.signal);
 async function run(name,input){const factory=tools.get(name);assert(factory);return factory.create(context).execute('synthetic',input,abort.signal);}
 return {workspace,note,state,paths,api,options,context,abort,client,operation,run,hooks,async close(){globalThis.fetch=original;state.current=false;await rm(workspace,{recursive:true,force:true});if(saved===undefined)delete process.env.SECRETARY_CORE_TOKEN;else process.env.SECRETARY_CORE_TOKEN=saved;}};
}
const discovery={document:'notes',mode:'discovery_summary'};

test('registered lifecycle revocation during task preflight prevents confirmation even while the host closure stays live',async()=>{
 const f=await fixture();try{
  f.operation('reset-grid').resource_kind='sheets_grid';
  const retained=f.client(),before=f.hooks.get('before_reset');assert(before);
  f.state.onExecute=()=>before({reason:'reset',messages:[]},f.context);
  await assert.rejects(retained.execute(id,1,'reset-grid'),/INVOCATION_EXPIRED/);
  assert.equal(f.state.current,true);assert.equal(f.state.sends,0);
  assert.equal(f.paths.filter(path=>path.endsWith('/confirm')).length,0);
  assert.throws(()=>f.client(),/INVOCATION_EXPIRED/);assert.deepEqual(await readdir(f.workspace),[]);
 }finally{await f.close();}
});

test('memory-aware native task fences before execute and confirm, then refreshes the same note for incremental edits',async()=>{
 const f=await fixture();try{
  assert.equal((await f.run('docs_read',discovery)).details.ok,true);const original=await readFile(f.note,'utf8');
  const client=f.client(),proposal=await client.propose(scope);await proposal.approval.onResolution('allow-once');const grant=await proposal.grant();
  let operationId;
  const fenced=async()=>{const bytes=await readFile(f.note,'utf8');assert(bytes.includes('"cache_status": "stale"'));assert(bytes.includes('"pending_operation_id": "'+operationId+'"'));};
  f.state.onExecute=fenced;f.state.onConfirm=fenced;
  for(operationId of ['op1','op2']){
   const result=await client.execute(grant.taskId,grant.revision,operationId);assert.equal(result.data.state,'applied');assert.equal(result.summary_memory.status,'refreshed');
  }
  assert.equal(f.state.sends,2);assert.equal(f.state.models,3);assert.equal(f.state.reads,3);assert.notEqual(await readFile(f.note,'utf8'),original);
  assert.deepEqual((await readdir(join(f.workspace,'memory/ciel-docs'))).map(name=>name.endsWith('.md')),[true]);
  assert.equal(f.paths.filter(path=>path.endsWith('/native')).length,2);
 }finally{await f.close();}
});

test('uncertain task member retains its fence and blocks forced discovery and a new member',async()=>{
 const f=await fixture();try{
  await f.run('docs_read',discovery);f.state.outcome='uncertain';const result=await f.client().execute(id,1,'uncertain-op');
  assert.equal(result.data.state,'uncertain');assert.equal(result.summary_memory.status,'stale');assert.equal(f.state.sends,1);
  assert.equal((await f.run('docs_read',{...discovery,refresh:true})).details.error.code,'SUMMARY_MEMORY_RECONCILIATION_REQUIRED');
  await assert.rejects(f.client().execute(id,1,'different-op'),/SUMMARY_MEMORY_RECONCILIATION_REQUIRED/);
  assert.equal(f.paths.filter(path=>path.endsWith('/execute')).length,1);assert.equal(f.state.models,1);
  f.operation('uncertain-op').state='applied';const observed=await f.run('operations_get_history',{operation_id:'uncertain-op'});
  assert.equal(observed.details.data.operation.state,'applied');assert.equal((await f.run('docs_read',discovery)).details.data.cache,'hit');assert.equal(f.state.sends,1);
 }finally{await f.close();}
});

test('lost native response retains stale evidence; observed history repairs memory without another dispatch',async()=>{
 const f=await fixture();try{
  f.state.onExecute=()=>{f.operation('lost-op').state='uncertain';throw Error('Synthetic lost response');};
  await assert.rejects(f.client().execute(id,1,'lost-op'),/TASK_OUTCOME_UNKNOWN/);assert((await readFile(f.note,'utf8')).includes('lost-op'));
  assert.equal((await f.run('docs_read',discovery)).details.error.code,'SUMMARY_MEMORY_RECONCILIATION_REQUIRED');
  f.operation('lost-op').state='applied';await f.run('operations_get_history',{operation_id:'lost-op'});
  assert.equal((await f.run('docs_read',discovery)).details.data.cache,'hit');assert.equal(f.paths.filter(path=>path.endsWith('/execute')).length,1);assert.equal(f.state.sends,0);
 }finally{await f.close();}
});

test('applied task result survives a failed model refresh and repairs only memory on discovery',async()=>{
 const f=await fixture();try{
  f.state.modelError=true;const result=await f.client().execute(id,1,'applied-op');
  assert.equal(result.data.state,'applied');assert.deepEqual(result.summary_memory,{status:'refresh_failed',document:'notes',source_write:'applied'});assert.equal(f.state.sends,1);
  f.state.modelError=false;assert.equal((await f.run('docs_read',discovery)).details.data.cache,'refreshed');assert.equal(f.state.sends,1);
 }finally{await f.close();}
});

test('a different operation or physical source cannot clear a retained task fence',async()=>{
 const f=await fixture();try{
  f.state.outcome='uncertain';await f.client().execute(id,1,'pending-op');const bytes=await readFile(f.note,'utf8'),op=f.operation('pending-op');op.state='applied';
  op.operation_id='different';assert.equal((await f.run('docs_read',discovery)).details.error.code,'SUMMARY_MEMORY_RECONCILIATION_REQUIRED');
  op.operation_id='pending-op';op.record_id='different-source';assert.equal((await f.run('docs_read',discovery)).details.error.code,'SUMMARY_MEMORY_RECONCILIATION_REQUIRED');
  assert.equal(await readFile(f.note,'utf8'),bytes);assert.equal(f.state.models,0);assert.equal(f.state.sends,1);
 }finally{await f.close();}
});

test('history identity and registry replacement failures deny native execution before note IO',async()=>{
 const f=await fixture();try{
  for(const key of ['operation_id','record_id','mapping_version','resource_kind']){
   const op=f.operation('op-'+key);op[key]='mismatch';
   await assert.rejects(f.client().execute(id,1,'op-'+key),/SUMMARY_MEMORY_(FENCE_REQUIRED|SOURCE_CHANGED)/);
  }
  f.state.registryId='replacement';await assert.rejects(f.client().execute(id,1,'replacement-op'),/SUMMARY_MEMORY_SOURCE_CHANGED/);
  f.state.registryId='doc';f.state.historyError=true;await assert.rejects(f.client().execute(id,1,'missing-op'),/SUMMARY_MEMORY_FENCE_REQUIRED/);
  assert.deepEqual(await readdir(f.workspace),[]);assert.equal(f.state.sends,0);assert.equal(f.paths.filter(path=>path.endsWith('/execute')).length,0);
 }finally{await f.close();}
});

test('a registered grid member executes without creating a Docs note or reporting a memory refresh',async()=>{
 const f=await fixture();try{
  f.operation('grid-op').resource_kind='sheets_grid';const result=await f.client().execute(id,1,'grid-op');
  assert.equal(result.data.state,'applied');assert.equal(result.summary_memory,undefined);assert.equal(f.state.sends,1);
  assert.equal(f.paths.filter(path=>path.endsWith('/docs_list')).length,0);assert.equal(f.state.models,0);assert.deepEqual(await readdir(f.workspace),[]);
 }finally{await f.close();}
});

test('memory audience loss at the final confirmation also fences a grid member without note IO',async()=>{
 const f=await fixture();try{
  f.operation('grid-revoked').resource_kind='sheets_grid';f.state.onExecute=()=>{f.state.audience=false;};
  await assert.rejects(f.client().execute(id,1,'grid-revoked'),/MEMORY_AUDIENCE_EXPIRED/);
  assert.equal(f.state.sends,0);assert.equal(f.paths.filter(path=>path.endsWith('/confirm')).length,0);assert.deepEqual(await readdir(f.workspace),[]);
 }finally{await f.close();}
});

test('unsafe note IO or an existing lock prevents native execution and preserves foreign evidence',async()=>{
 const f=await fixture(),outside=await mkdtemp(join(tmpdir(),'ciel-task-outside-'));try{
  await mkdir(join(f.workspace,'memory'));await symlink(outside,join(f.workspace,'memory/ciel-docs'));
  await assert.rejects(f.client().execute(id,1,'unsafe-op'));assert.deepEqual(await readdir(outside),[]);
  await rm(join(f.workspace,'memory/ciel-docs'));await f.run('docs_read',discovery);await writeFile(f.note+'.lock','foreign-lock');
  await assert.rejects(f.client().execute(id,1,'busy-op'),/SUMMARY_MEMORY_BUSY/);assert.equal(await readFile(f.note+'.lock','utf8'),'foreign-lock');
  assert.equal(f.paths.filter(path=>path.endsWith('/execute')).length,0);
 }finally{await f.close();await rm(outside,{recursive:true,force:true});}
});

test('memory audience loss during Core preflight denies confirmation and retains the stale note',async()=>{
 const f=await fixture();try{
  f.state.onExecute=()=>{f.state.audience=false;};await assert.rejects(f.client().execute(id,1,'revoked-op'),/MEMORY_AUDIENCE_EXPIRED/);
  assert.equal(f.state.sends,0);assert.equal(f.paths.filter(path=>path.endsWith('/confirm')).length,0);assert((await readFile(f.note,'utf8')).includes('revoked-op'));
 }finally{await f.close();}
});

test('a delayed or denied grant never reaches history or native note fencing',async()=>{
 const f=await fixture();try{
  const client=f.client(),proposal=await client.propose(scope),decision=deferred(),started=deferred();f.state.onDecision=async()=>{started.resolve();await decision.promise;};
  const resolution=proposal.approval.onResolution('deny');await started.promise;
  const executing=client.execute(id,1,'denied-op');await new Promise(resolve=>setImmediate(resolve));assert.equal(f.paths.length,2);assert.deepEqual(await readdir(f.workspace),[]);
  decision.resolve();await resolution;await assert.rejects(executing,/APPROVAL_DENIED/);assert.equal(f.paths.length,2);assert.equal(f.state.sends,0);
 }finally{await f.close();}
});

test('memory composition requires explicit opt-in, paired owner, private authority and native qualification',async()=>{
 const f=await fixture();try{
  assert.throws(()=>createMemoryNativeTaskClient({...f.options,summaryMemoryEnabled:false},f.api,f.context,f.abort.signal),/SUMMARY_MEMORY_DISABLED/);
  assert.throws(()=>createMemoryNativeTaskClient({...f.options,owner:'different'},f.api,f.context,f.abort.signal),/INVALID_CONFIGURATION/);
  assert.throws(()=>createMemoryNativeTaskClient({...f.options,qualified:false},f.api,f.context,f.abort.signal),/TASK_TRANSPORT_UNQUALIFIED/);
  assert.throws(()=>createMemoryNativeTaskClient(f.options,f.api,{...f.context,memoryAudience:undefined},f.abort.signal),/SUMMARY_MEMORY_AUTHORITY_REQUIRED/);
  f.api.pluginConfig.readOnlyRuntime=true;assert.throws(()=>f.client(),/INVALID_CONFIGURATION/);assert.equal(f.paths.length,0);assert.deepEqual(await readdir(f.workspace),[]);
 }finally{await f.close();}
});

test('an applied cleanup tombstone resolves a stale native note through permanent history, without another write',async()=>{
 const f=await fixture();try{
  f.state.modelError=true;
  const result=await f.client().execute(id,1,'cleaned-op');
  assert.equal(result.data.state,'applied');assert.equal(result.summary_memory.status,'refresh_failed');
  const stale=await readFile(f.note,'utf8');assert(stale.includes('"pending_operation_id": "cleaned-op"'));
  // Exact identity/outcome projection retained by the real Core cleanup route;
  // this SDK IO fixture does not remove a real operator journal or personal note.
  const op=f.operation('cleaned-op');op.evidence={status:'unavailable',cleanup_id:'33333333-3333-4333-8333-333333333333',removed_at:1};
  for(const key of ['before','after','fields','source_revision','expires_at'])delete op[key];
  f.state.modelError=false;
  const refreshed=await f.run('docs_read',discovery);
  assert.equal(refreshed.details.data.cache,'refreshed');assert.equal(f.state.sends,1);
  assert.equal(f.paths.filter(path=>path.endsWith('/execute')).length,1);
  assert(!(await readFile(f.note,'utf8')).includes('pending_operation_id'));
 }finally{await f.close();}
});
