/** Built default model tools -> normal Core startup -> journal -> simulated Google.
 * Host requester/approval delivery and optional model output are controlled inputs.
 */
import assert from 'node:assert/strict';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import entry from '../plugins/secretary-tools/dist/index.js';
import {acceptanceHooks} from './acceptance-hooks.mjs';
const [coreUrl,docsUrl,sheetsUrl]=process.argv.slice(2);
for(const [value,path] of [[coreUrl,'/'],[docsUrl,'/google-docs/v1'],[sheetsUrl,'/google/v4']]){
 const url=new URL(value);assert.equal(url.protocol,'http:');assert.equal(url.hostname,'127.0.0.1');assert.equal(url.pathname,path);assert(!url.username&&!url.password&&!url.search&&!url.hash);
}
process.env.SECRETARY_CORE_TOKEN='synthetic-acceptance-core-token';process.env.SECRETARY_APPROVAL_TOKEN='synthetic-separate-operator-token';
const owner='+15555550123',workspace=await mkdtemp(join(tmpdir(),'ciel-task-tool-acceptance-')),hooks=acceptanceHooks(),tools=new Map();let prompts=0,models=0;
const context={agentId:'synthetic-agent',workspaceDir:workspace,sessionKey:'agent:synthetic-agent:main',sessionId:'task-incarnation',requesterSenderId:owner,senderIsOwner:true,sandboxed:false,memoryAudience:{kind:'owner-private',agentId:'synthetic-agent'},assertInvocationCurrent(){},assertMemoryAudienceCurrent(){}};
const api={pluginConfig:{owner,coreUrl,approvalTransportQualified:true,taskToolsEnabled:true,taskTransportQualified:true,summaryMemoryEnabled:true},config:{plugins:{entries:{'memory-core':{config:{dreaming:{enabled:false}}}}}},runtime:{gateway:{isAvailable:()=>false},llm:{async complete(){models++;return {text:JSON.stringify({summary:'Synthetic notes record the current bounded sample [S1].',excerpt_ids:['S1']})};}}},registerTool(factory,options){tools.set(options.name,factory);},on:hooks.on};
entry.register(api);const abort=new AbortController(),instances=new Map([...tools].map(([name,factory])=>[name,factory.create(context)]));
async function run(name,input){const result=await instances.get(name).execute('task-tool-acceptance',input,abort.signal);assert.deepEqual(JSON.parse(result.content[0].text),result.details);return result.details;}
async function member(name,input){assert.equal(await hooks.beforeToolCall({toolName:name,params:input},{...context,abortSignal:abort.signal,requester:{senderId:owner,senderIsOwner:true}}),undefined);return run(name,input);}
async function fixture(base,path,body){const response=await fetch(base.replace(/\/v[14]$/,'')+path,{method:body===undefined?'GET':'POST',headers:{'content-type':'application/json'},body:body===undefined?undefined:JSON.stringify(body),signal:AbortSignal.timeout(5000)});assert(response.ok);return response.json();}
try{
 let ready=false;for(let attempt=0;attempt<50;attempt++){
  try{if((await run('docs_list',{})).ok){ready=true;break;}}catch(error){if(attempt===49)throw error;}
  await new Promise(resolve=>setTimeout(resolve,200));
 }assert(ready,'Scoped Core startup failed');
 await fixture(docsUrl,'/__fixture/reset',{});
 const gridBefore=await fixture(sheetsUrl,'/__fixture/stats');
 const discovered=await run('docs_read',{document:'notes',mode:'discovery_summary'});assert.equal(discovered.data.cache,'refreshed');assert.equal(models,1);
 const scope={objective:'Correct the sample note and literal grid cells',mapping_version:'generic-workspace-v1',max_operations:6,targets:[{kind:'document',document:'notes',document_id:'demo-document',tab_id:'t.0'},{kind:'grid',spreadsheet:'operations',spreadsheet_id:'generic-spreadsheet',view:'overview',sheet_id:7,range:'B4:D8'}]};
 const input={scope},decision=await hooks.beforeToolCall({toolName:'tasks_propose',params:input},{...context,abortSignal:abort.signal,requester:{senderId:owner,senderIsOwner:true}});assert(decision.requireApproval);prompts++;
 await decision.requireApproval.onResolution('allow-once');const grant=await run('tasks_propose',input);assert.equal(grant.ok,true);
 const task_id=grant.data.task_id,grant_revision=grant.data.grant_revision;
 // Core's task profile must close the legacy mutation/approval bypass.
 const legacy=await run('docs_preview_change',{operation_id:'task-legacy-bypass',document:'notes',tab_id:'t.0',action:'replace_text',fields:{find:'sample',replacement:'wrong'}});assert.equal(legacy.error.code,'task_dispatch_required');
 const docsChange=(operation_id,find,replacement)=>({kind:'document',operation_id,document:'notes',tab_id:'t.0',action:'replace_text',fields:{find,replacement}});
 for(const [operation_id,find,replacement] of [['task-doc-one','sample','team'],['task-doc-two','team','final']]){
  assert.equal((await member('tasks_capture',{task_id,change:docsChange(operation_id,find,replacement)})).data.state,'previewed');
  const result=await member('tasks_execute',{task_id,grant_revision,operation_id});assert.equal(result.data.state,'applied');assert.equal(result.summary_memory.status,'refreshed');
  assert.equal((await member('tasks_execute',{task_id,grant_revision,operation_id})).ok,false);
 }
 assert.equal((await fixture(docsUrl,'/__fixture/stats')).applies,2);assert.equal(models,3);
 const grid={kind:'grid',operation_id:'task-grid-one',spreadsheet:'operations',view:'overview',action:'update',fields:{range:'B7:C7',values:[['Task literal',false]]}};
 assert.equal((await member('tasks_capture',{task_id,change:grid})).data.state,'previewed');
 assert.equal((await member('tasks_execute',{task_id,grant_revision,operation_id:grid.operation_id})).data.state,'applied');
 assert.equal((await fixture(sheetsUrl,'/__fixture/stats')).gridWrites,gridBefore.gridWrites+1);assert.equal(models,3);
 const lost=docsChange('task-doc-lost','final','observed');await member('tasks_capture',{task_id,change:lost});
 await fixture(docsUrl,'/__fixture/lose-next-response',{});
 const uncertain=await member('tasks_execute',{task_id,grant_revision,operation_id:lost.operation_id});assert.equal(uncertain.data.state,'uncertain');
 assert.equal((await member('tasks_execute',{task_id,grant_revision,operation_id:lost.operation_id})).ok,false);
 assert.equal((await fixture(docsUrl,'/__fixture/stats')).applies,3);
 assert.equal((await run('docs_read',{document:'notes',mode:'discovery_summary'})).error.code,'SUMMARY_MEMORY_RECONCILIATION_REQUIRED');
 const response=await fetch(coreUrl+'/v1/operations/'+lost.operation_id+'/reconcile',{method:'POST',headers:{authorization:'Bearer synthetic-separate-operator-token','content-type':'application/json'},body:'{}',signal:AbortSignal.timeout(5000)});assert.equal((await response.json()).data.state,'applied');
 const history=await run('operations_get_history',{operation_id:lost.operation_id});assert.equal(history.data.operation.state,'applied');
 const observed=await member('tasks_inspect',{task_id});assert.equal(observed.data.phase,'suspended');assert.equal(observed.data.admitted_operations,4);
 assert.equal((await member('tasks_end',{task_id,grant_revision:observed.data.grant_revision,reason:'cancelled'})).data.phase,'closed');
 assert.equal(prompts,1);assert.equal(models,4);
 console.log(JSON.stringify({acceptance:'passed',scenario:'native-task-model-tools',owner_prompts:prompts,docs_sends:3,grid_sends:1,scope:'built model tools/normal startup/Core/journal/native note IO with simulated Google/model; host authority/owner routing are synthetic, not live qualification'}));
}finally{await rm(workspace,{recursive:true,force:true});}
