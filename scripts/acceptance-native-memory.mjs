/** Real pinned Node/SDK + native file IO + Core/journal; only Google and model output are simulated. */
import assert from 'node:assert/strict';
import {mkdtemp,rm,readdir,stat} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import entry from '../plugins/secretary-tools/dist/index.js';
const [coreUrl,docsUrl]=process.argv.slice(2);
for(const [value,path] of [[coreUrl,'/'],[docsUrl,'/google-docs/v1']]){
 const url=new URL(value);assert.equal(url.protocol,'http:');assert.equal(url.hostname,'127.0.0.1');assert.equal(url.pathname,path);assert(!url.username&&!url.password&&!url.search&&!url.hash);
}
assert.equal(process.env.SECRETARY_CORE_TOKEN,'synthetic-acceptance-core-token');
process.env.SECRETARY_APPROVAL_TOKEN='synthetic-separate-operator-token';
const workspace=await mkdtemp(join(tmpdir(),'ciel-native-acceptance-'));
let models=0,hook;
const tools=new Map(),owner='+15555550123';
const context={agentId:'synthetic-agent',workspaceDir:workspace,sessionKey:'agent:synthetic-agent:main',requesterSenderId:owner,senderIsOwner:true,sandboxed:false,memoryAudience:{kind:'owner-private',agentId:'synthetic-agent'},assertInvocationCurrent(){},assertMemoryAudienceCurrent(){}};
const api={pluginConfig:{owner,coreUrl,approvalTransportQualified:true,summaryMemoryEnabled:true},config:{plugins:{entries:{'memory-core':{config:{dreaming:{enabled:false}}}}}},runtime:{gateway:{isAvailable:()=>false},llm:{async complete(){models++;return {text:JSON.stringify({summary:'Synthetic registered notes contain the recorded sample and subsequent approved edits [S1].',excerpt_ids:['S1']})};}}},registerTool(factory,options){tools.set(options.name,factory);},on(name,handler){assert.equal(name,'before_tool_call');hook=handler;}};
entry.register(api);
async function run(name,input){const result=await tools.get(name)(context).execute('native-memory-acceptance',input);assert.deepEqual(JSON.parse(result.content[0].text),result.details);return result;}
async function approve(change){const preview=await run('docs_preview_change',change);assert.equal(preview.details.ok,true);const decision=await hook({toolName:'docs_apply',params:{operation_id:change.operation_id}},{requester:{senderId:owner,senderIsOwner:true}});assert(!decision.block);await decision.requireApproval.onResolution('allow-once');}
const discovery={document:'notes',mode:'discovery_summary'};
try{
 const first=await run('docs_read',discovery);assert.equal(first.details.ok,true,JSON.stringify(first.details));assert.equal(first.details.data.cache,'refreshed');assert.equal(models,1);
 assert.equal((await run('docs_read',discovery)).details.data.cache,'hit');assert.equal(models,1);
 const change={operation_id:'acceptance-native-memory',document:'notes',tab_id:'t.0',action:'append_text',fields:{text:' Native memory edit'}};
 await approve(change);
 const applied=await run('docs_apply',{operation_id:change.operation_id});assert.equal(applied.details.data.state,'applied');assert.equal(JSON.parse(applied.content[1].text).summary_memory.status,'refreshed');
 const after=await run('docs_read',discovery);assert.equal(after.details.data.cache,'hit');assert.notEqual(after.details.data.source.source_fingerprint,first.details.data.source.source_fingerprint);assert.equal(models,2);
 const lost={...change,operation_id:'acceptance-native-memory-lost',fields:{text:' Native observed edit'}};
 await approve(lost);
 const fixture=await fetch(docsUrl.replace(/\/v1$/,'')+'/__fixture/lose-next-response',{method:'POST',headers:{'content-type':'application/json'},body:'{}',signal:AbortSignal.timeout(5000)});assert(fixture.ok);
 const uncertain=await run('docs_apply',{operation_id:lost.operation_id});assert.equal(uncertain.details.data.state,'uncertain');
 assert.equal((await run('docs_read',discovery)).details.error.code,'SUMMARY_MEMORY_RECONCILIATION_REQUIRED');assert.equal(models,2);
 const reconciled=await fetch(`${coreUrl}/v1/operations/${lost.operation_id}/reconcile`,{method:'POST',headers:{authorization:'Bearer synthetic-separate-operator-token','content-type':'application/json'},body:'{}',signal:AbortSignal.timeout(5000)});assert.equal((await reconciled.json()).data.state,'applied');
 const observed=await run('operations_get_history',{operation_id:lost.operation_id});assert.equal(observed.details.data.operation.state,'applied');assert.equal(JSON.parse(observed.content[1].text).summary_memory.status,'refreshed');
 const current=await run('docs_read',discovery);assert.equal(current.details.data.cache,'hit');assert.equal(models,3);
 const files=(await readdir(join(workspace,'memory/ciel-docs'))).filter(n=>n.endsWith('.md'));assert.equal(files.length,1);assert.equal((await stat(join(workspace,'memory/ciel-docs',files[0]))).mode&0o777,0o600);
 console.log(JSON.stringify({acceptance:'passed',scenario:'native-summary-memory',scope:'real Node/SDK/native file IO/Core/journal with synthetic model response and Google; not live model/host/index qualification'}));
}finally{await rm(workspace,{recursive:true,force:true});}
