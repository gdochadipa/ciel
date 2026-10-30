import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {mkdtemp,rm,readFile,readdir,stat,writeFile,mkdir,symlink} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import entry from '../dist/index.js';

async function fixture() {
 const workspace=await mkdtemp(join(tmpdir(),'ciel-native-memory-'));
 const saved=process.env.SECRETARY_CORE_TOKEN,savedApproval=process.env.SECRETARY_APPROVAL_TOKEN;process.env.SECRETARY_CORE_TOKEN='synthetic';process.env.SECRETARY_APPROVAL_TOKEN='synthetic-approval';
 const state={reads:0,models:0,lists:0,version:'v1',readable:true,text:'A short original document\n',modelError:false,writes:0,applyState:'applied',historyState:'approved',onApply:undefined,modelText:undefined,onModel:undefined,onRead:undefined};
 const server=createServer(async (req,res)=>{
  const chunks=[];for await(const chunk of req)chunks.push(chunk);
  const request=new Request('http://127.0.0.1'+req.url,{method:'POST',body:Buffer.concat(chunks)});
  const response=await handle(request);res.writeHead(response.status,{'content-type':'application/json'});res.end(await response.text());
 });
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 async function handle(request){
  const name=new URL(request.url).pathname.split('/').at(-1),input=await request.json();
  if(name==='docs_list'){state.lists++;return Response.json({ok:true,data:{mapping_version:state.version,documents:state.readable?[{document:'notes',id:'synthetic-doc',readable:true}]:[]}});}
  if(name==='operations_get_history')return Response.json({ok:true,data:{operation:{resource_kind:'google_docs',document:'notes',state:state.historyState}}});
  if(name==='docs_apply'){state.writes++;await state.onApply?.();state.text='Updated document after approved edit\n';state.historyState=state.applyState;return Response.json({ok:true,data:{state:state.applyState}});}
  if(name==='docs_read'){
   state.reads++;const n=[...state.text].length;
   const packet={mode:'summary_context',content_role:'untrusted_source_data',context_id:'b'.repeat(64),source:{document:'notes',document_id:'synthetic-doc',title:'Test',revision_id:'rev1',source_fingerprint:createHash('sha256').update(state.text).digest('hex'),mapping_version:state.version,retrieved_at:new Date().toISOString()},tabs:[{tab_id:'t.0',title:'Main',total_characters:n,included_characters:n,truncated:false,excerpt_id:'S1'}],excerpts:[{excerpt_id:'S1',text:state.text,source:{tab_id:'t.0',start_character:0,end_character:n,next_cursor:null}}],coverage:{scope:'projected_body_text_only',total_characters:n,included_characters:n,omitted_characters:0,total_tabs:1,included_tabs:1,projected_text_complete:true,can_summarize:true}};
   await state.onRead?.(input,packet);return Response.json({ok:true,data:packet});
  }
  return Response.json({ok:false,error:{code:'unexpected'}});
 }
 let current=true;
 const context={agentId:'agent-one',workspaceDir:workspace,sessionKey:'agent:agent-one:main',requesterSenderId:'owner',senderIsOwner:true,memoryAudience:{kind:'owner-private',agentId:'agent-one'},assertInvocationCurrent(){if(!current)throw Error('INVOCATION_EXPIRED');},assertMemoryAudienceCurrent(){if(!current)throw Error('INVOCATION_EXPIRED');},sandboxed:false};
 function registry(config={summaryMemoryEnabled:true},hostConfig={plugins:{entries:{'memory-core':{config:{dreaming:{enabled:false}}}}}}) {
  const tools=new Map();
  const api={pluginConfig:{owner:'owner',approvalTransportQualified:true,coreUrl:'http://127.0.0.1:'+server.address().port,...config},config:hostConfig,runtime:{gateway:{isAvailable:()=>false},llm:{async complete(params){state.models++;await state.onModel?.();if(state.modelError)throw Error('private model error');assert.equal(params.model,undefined);return {text:state.modelText??JSON.stringify({summary:'The document gives an original short description [S1].',excerpt_ids:['S1']})};}}},registerTool(factory,options){tools.set(options.name,factory);},on(){}};
  entry.register(api);
  return {async run(input,ctx=context,name='docs_read'){const factory=tools.get(name);const tool=factory(ctx);return await tool.execute('synthetic',input);}};
 }
 return {workspace,state,context,registry,revoke(){current=false;},async close(){server.closeAllConnections();await new Promise(resolve=>server.close(resolve));await rm(workspace,{recursive:true,force:true});if(saved===undefined)delete process.env.SECRETARY_CORE_TOKEN;else process.env.SECRETARY_CORE_TOKEN=saved;if(savedApproval===undefined)delete process.env.SECRETARY_APPROVAL_TOKEN;else process.env.SECRETARY_APPROVAL_TOKEN=savedApproval;}};
}

test('actual SDK factory saves one native initial summary and reuses it across bridge recreation without another source/model call',async()=>{
 const f=await fixture();try{
  const first=await f.registry().run({document:'notes',mode:'discovery_summary'});
  assert.equal(first.details.ok,true,JSON.stringify(first.details));assert.equal(first.details.data.cache,'refreshed');
  assert(first.details.data.summary.includes('[S1]'));
  assert.equal(Date.parse(first.details.data.expires_at)-Date.parse(first.details.data.source.retrieved_at),7*86400000);
  assert.equal(f.state.reads,1);assert.equal(f.state.models,1);
  const second=await f.registry().run({document:'notes',mode:'discovery_summary'});
  assert.equal(second.details.data.cache,'hit');assert.equal(f.state.reads,1);assert.equal(f.state.models,1);
  const files=(await readdir(join(f.workspace,'memory/ciel-docs'))).filter(n=>n.endsWith('.md'));
  assert.equal(files.length,1);const path=join(f.workspace,'memory/ciel-docs',files[0]);
  assert.equal((await stat(path)).mode&0o777,0o600);
  assert(!(await readFile(path,'utf8')).includes(f.state.text));
 }finally{await f.close();}
});


async function notePath(f){return join(f.workspace,'memory/ciel-docs',(await readdir(join(f.workspace,'memory/ciel-docs'))).find(n=>n.endsWith('.md')));}
const discoveryInput={document:'notes',mode:'discovery_summary'};

test('native discovery expires exactly at seven days, never slides on reads, and refreshes only current registered sources',async t=>{
 t.mock.timers.enable({apis:['Date'],now:Date.now()});
 const f=await fixture();try{
  const api=f.registry(),first=await api.run(discoveryInput),expiry=Date.parse(first.details.data.expires_at);
  t.mock.timers.setTime(expiry-1);
  assert.equal((await api.run(discoveryInput)).details.data.cache,'hit');assert.equal(f.state.models,1);
  assert.equal((await api.run(discoveryInput)).details.data.expires_at,first.details.data.expires_at);
  t.mock.timers.setTime(expiry);
  const refreshed=await api.run(discoveryInput);assert.equal(refreshed.details.data.cache,'refreshed');assert.equal(f.state.models,2);
  assert.equal(Date.parse(refreshed.details.data.source.retrieved_at),expiry);
  f.state.readable=false;assert.equal((await api.run(discoveryInput)).details.error.code,'DOCUMENT_NOT_REGISTERED');assert.equal(f.state.models,2);
  f.state.readable=true;f.state.version='v2';assert.equal((await api.run(discoveryInput)).details.data.cache,'refreshed');assert.equal(f.state.models,3);
 }finally{await f.close();}
});

test('ordinary summary reads save native notes and forced discovery refresh replaces the same note with new evidence',async()=>{
 const f=await fixture();try{
  const api=f.registry();const first=await api.run({document:'notes',mode:'summary_context'});
  assert.equal(first.details.data.mode,'summary_context');assert.equal(JSON.parse(first.content[1].text).summary_memory.status,'complete');
  const original=await api.run(discoveryInput);assert.equal(original.details.data.cache,'hit');assert.equal(f.state.models,1);
  f.state.text='External change in Google\n';
  assert.equal((await api.run(discoveryInput)).details.data.cache,'hit');
  const forced=await api.run({...discoveryInput,refresh:true});assert.equal(forced.details.data.cache,'refreshed');assert.notEqual(forced.details.data.source.source_fingerprint,original.details.data.source.source_fingerprint);
  assert.equal((await readdir(join(f.workspace,'memory/ciel-docs'))).filter(n=>n.endsWith('.md')).length,1);
 }finally{await f.close();}
});

test('an uncertain Docs apply fences the native note until observed reconciliation, without a second write',async()=>{
 const f=await fixture();try{
  const api=f.registry();await api.run(discoveryInput);
  f.state.applyState='uncertain';
  f.state.onApply=async()=>{assert((await readFile(await notePath(f),'utf8')).includes('"cache_status": "stale"'));};
  const applied=await api.run({operation_id:'op1'},f.context,'docs_apply');assert.equal(applied.details.data.state,'uncertain');assert.equal(f.state.writes,1);
  const held=await api.run(discoveryInput);assert.equal(held.details.ok,false);assert.equal(held.details.error.code,'SUMMARY_MEMORY_RECONCILIATION_REQUIRED');assert.equal(f.state.models,1);
  f.state.historyState='applied';
  const history=await api.run({operation_id:'op1'},f.context,'operations_get_history');assert.equal(history.details.data.operation.state,'applied');
  const current=await api.run(discoveryInput);assert.equal(current.details.data.cache,'hit');assert.equal(f.state.models,2);assert.equal(f.state.writes,1);
 }finally{await f.close();}
});


test('discovery enforces owner, native audience, lifetime, opt-in and strict input before source/model/storage effects',async()=>{
 const f=await fixture();try{
  const disabled=f.registry({summaryMemoryEnabled:false});assert.equal((await disabled.run(discoveryInput)).details.error.code,'SUMMARY_MEMORY_DISABLED');
  const api=f.registry();
  for(const input of [{...discoveryInput,ttl:1},{...discoveryInput,refresh:null},{...discoveryInput,documents:['notes']},{...discoveryInput,path:'/tmp/other'},{...discoveryInput,model:'new-paid-model'},{...discoveryInput,max_characters:300}])assert.equal((await api.run(input)).details.error.code,'INVALID_INPUT');
  assert.equal((await api.run(discoveryInput,{...f.context,senderIsOwner:false})).details.error.code,'OWNER_REQUIRED');
  assert.equal((await api.run(discoveryInput,{...f.context,memoryAudience:{kind:'conversation',agentId:'agent-one'}})).details.error.code,'SUMMARY_MEMORY_AUTHORITY_REQUIRED');
  assert.equal((await api.run(discoveryInput,{...f.context,sandboxed:true})).details.error.code,'SUMMARY_MEMORY_AUTHORITY_REQUIRED');
  assert.equal((await api.run(discoveryInput,{...f.context,assertMemoryAudienceCurrent:undefined})).details.error.code,'SUMMARY_MEMORY_AUTHORITY_REQUIRED');
  f.revoke();assert.equal((await api.run(discoveryInput)).details.error.code,'INVOCATION_EXPIRED');
  assert.equal(f.state.lists,0);assert.equal(f.state.reads,0);assert.equal(f.state.models,0);assert.equal((await readdir(f.workspace)).length,0);
 }finally{await f.close();}
});

test('native IO rejects symlinked note directories and retained foreign locks without touching their targets',async()=>{
 const f=await fixture(),outside=await mkdtemp(join(tmpdir(),'ciel-outside-'));try{
  await mkdir(join(f.workspace,'memory'));await symlink(outside,join(f.workspace,'memory/ciel-docs'));
  const api=f.registry();assert.equal((await api.run(discoveryInput)).details.ok,false);assert.equal((await readdir(outside)).length,0);assert.equal(f.state.models,0);
  await rm(join(f.workspace,'memory/ciel-docs'));
  const afterRemoval=await api.run(discoveryInput);assert.equal(afterRemoval.details.ok,true,JSON.stringify(afterRemoval.details));
  const lock=(await notePath(f))+'.lock';await writeFile(lock,'foreign-lock');
  assert.equal((await api.run(discoveryInput)).details.error.code,'SUMMARY_MEMORY_BUSY');assert.equal(await readFile(lock,'utf8'),'foreign-lock');assert.equal(f.state.models,1);
 }finally{await f.close();await rm(outside,{recursive:true,force:true});}
});

test('bad model citations, errors and revoked in-flight native authority never publish a fresh note',async()=>{
 const f=await fixture();try{
  const api=f.registry();f.state.modelText=JSON.stringify({summary:'Invented evidence [S999].',excerpt_ids:['S999']});
  assert.equal((await api.run(discoveryInput)).details.error.code,'SUMMARY_MEMORY_MODEL_FAILED');
  assert.equal((await readdir(join(f.workspace,'memory/ciel-docs'))).filter(n=>n.endsWith('.md')).length,0);
  f.state.modelText=undefined;f.state.modelError=true;
  const failed=await api.run(discoveryInput);assert.equal(failed.details.error.code,'SUMMARY_MEMORY_MODEL_FAILED');assert(!JSON.stringify(failed).includes('private model error'));
  f.state.modelError=false;f.state.onModel=()=>f.revoke();
  assert.equal((await api.run(discoveryInput)).details.error.code,'INVOCATION_EXPIRED');
  assert.equal((await readdir(join(f.workspace,'memory/ciel-docs'))).filter(n=>n.endsWith('.md')).length,0);
 }finally{await f.close();}
});

test('an applied edit with a failed native refresh stays applied and stale, then repairs memory without resending the write',async()=>{
 const f=await fixture();try{
  const api=f.registry();await api.run(discoveryInput);
  const old=(await api.run(discoveryInput)).details.data.source.source_fingerprint;
  f.state.onApply=()=>{f.state.modelError=true;};
  const applied=await api.run({operation_id:'op1'},f.context,'docs_apply');
  assert.equal(applied.details.data.state,'applied');assert.equal(JSON.parse(applied.content[1].text).summary_memory.status,'refresh_failed');assert.equal(f.state.writes,1);
  assert((await readFile(await notePath(f),'utf8')).includes('"cache_status": "stale"'));
  f.state.modelError=false;
  const repaired=await api.run(discoveryInput);assert.equal(repaired.details.data.cache,'refreshed');assert.notEqual(repaired.details.data.source.source_fingerprint,old);assert.equal(f.state.writes,1);
 }finally{await f.close();}
});

test('native feature failure preserves ordinary source reads and reports the missing memory step separately',async()=>{
 const f=await fixture();try{
  const result=await f.registry().run({document:'notes',mode:'summary_context'},{...f.context,memoryAudience:undefined});
  assert.equal(result.details.ok,true);assert.equal(result.details.data.mode,'summary_context');assert.equal(JSON.parse(result.content[1].text).summary_memory.status,'unavailable');assert.equal(f.state.models,0);
 }finally{await f.close();}
});


test('malformed/future native records are misses and do not become trusted source notes',async()=>{
 const f=await fixture();try{
  const api=f.registry();await api.run(discoveryInput);const path=await notePath(f),raw=await readFile(path,'utf8');
  const start=raw.indexOf('```json\n')+8,end=raw.lastIndexOf('\n```'),note=JSON.parse(raw.slice(start,end));
  note.expires_at='not-a-date';await writeFile(path,raw.slice(0,start)+JSON.stringify(note)+raw.slice(end));
  assert.equal((await api.run(discoveryInput)).details.data.cache,'refreshed');assert.equal(f.state.models,2);
  const current=await readFile(path,'utf8'),n=JSON.parse(current.slice(current.indexOf('```json\n')+8,current.lastIndexOf('\n```')));
  n.source.retrieved_at=new Date(Date.now()+86400000).toISOString();n.expires_at=new Date(Date.now()+8*86400000).toISOString();
  await writeFile(path,current.slice(0,current.indexOf('```json\n')+8)+JSON.stringify(n)+current.slice(current.lastIndexOf('\n```')));
  assert.equal((await api.run(discoveryInput)).details.data.cache,'refreshed');assert.equal(f.state.models,3);
 }finally{await f.close();}
});

test('a declared remote workspace never falls back to local native note writes',async()=>{
 const f=await fixture();const {registerAgentWorkspaceAccess}=await import('openclaw/plugin-sdk/agent-workspace-runtime');
 const stop=registerAgentWorkspaceAccess(f.workspace,{bridge:{}});
 try{const result=await f.registry().run(discoveryInput);assert.equal(result.details.error.code,'SUMMARY_MEMORY_REMOTE_UNSUPPORTED');assert.equal(f.state.models,0);assert.equal((await readdir(f.workspace)).length,0);}
 finally{stop();await f.close();}
});


test('native summary notes require explicit no-promotion policy and never enable or replace a host memory provider',async()=>{
 const f=await fixture();try{
  const result=await f.registry({summaryMemoryEnabled:true},{}).run(discoveryInput);assert.equal(result.details.error.code,'SUMMARY_MEMORY_POLICY_REQUIRED');assert.equal(f.state.models,0);assert.equal(f.state.lists,0);assert.equal((await readdir(f.workspace)).length,0);
 }finally{await f.close();}
});


test('a delayed pre-edit source packet cannot overwrite the refreshed post-edit native note',async()=>{
 const f=await fixture();let release;try{
  const api=f.registry();await api.run(discoveryInput);
  let captured;const arrived=new Promise(resolve=>{captured=resolve;});const paused=new Promise(resolve=>{release=resolve;});
  f.state.onRead=async()=>{f.state.onRead=undefined;captured();await paused;};
  const delayed=api.run({document:'notes',mode:'summary_context'});await arrived;
  const applied=await api.run({operation_id:'op-race'},f.context,'docs_apply');assert.equal(applied.details.data.state,'applied');
  const current=await api.run(discoveryInput),expected=current.details.data.source.source_fingerprint;
  release();await delayed;
  const after=await api.run(discoveryInput);assert.equal(after.details.data.source.source_fingerprint,expected);assert.equal(f.state.writes,1);
 }finally{release?.();await f.close();}
});
