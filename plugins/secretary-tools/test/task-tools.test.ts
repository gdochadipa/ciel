import {test,expect} from 'bun:test';
import {taskToolInput} from '../src/task-tool-specs';
import {TaskToolApprovals,executeTaskTool} from '../src/task-tools';
import type {NativeTaskClient,NativeTaskProposal} from '../src/task-native';
const id='11111111-1111-4111-8111-111111111111';
const scope={objective:'Correct note',mapping_version:'v1',max_operations:4,targets:[{kind:'document',document:'notes',document_id:'doc',tab_id:'t.0'}]};
function fixture(){
 let active=true,resolve:((value:void)=>void)|undefined,decision='pending',proposals=0;
 const ack=new Promise<void>(done=>{resolve=done;});
 const context={agentId:'a',sessionKey:'s',sessionId:'i',assertInvocationCurrent(){if(!active)throw Error('INVOCATION_EXPIRED');}};
 const proposal:NativeTaskProposal={taskId:id,approval:{title:'scope',description:'scope',severity:'warning',allowedDecisions:['allow-once','deny'],timeoutMs:60000,async onResolution(value){decision=value;await ack;}},async grant(){await ack;if(decision!=='allow-once')throw Error('APPROVAL_DENIED');context.assertInvocationCurrent();return {taskId:id,scopeDigest:'a'.repeat(64),revision:1};}};
 const calls:unknown[]=[];
 const client:NativeTaskClient={async propose(input){proposals++;calls.push(input);return proposal;},async resume(task){calls.push(task);return proposal;},async inspect(task){calls.push(['inspect',task]);return {phase:'active'};},async capture(task,input){calls.push(['capture',task,input]);return {state:'previewed'};},async discard(task,op){calls.push(['discard',task,op]);return {state:'rejected'};},async execute(task,revision,op){calls.push(['execute',task,revision,op]);return {ok:true,data:{state:'uncertain'}};},async end(task,revision,reason){calls.push(['end',task,revision,reason]);return {phase:'closed'};}};
 return {context,client,calls,approvals:new TaskToolApprovals(),proposal,proposals:()=>proposals,ack(){if(resolve===undefined)throw Error('Missing resolver');resolve();},revoke(){active=false;}};
}
test('model task input snapshots intent and rejects extra authority, revision and selector fields',()=>{
 const original={scope:structuredClone(scope)},captured=taskToolInput('tasks_propose',original);original.scope.objective='changed';expect(captured.scope).toEqual(scope);
 for(const input of [{task_id:id,owner:'injected'},{task_id:id,context:{}},{task_id:'wrong'},{}])expect(()=>taskToolInput('tasks_inspect',input)).toThrow('INVALID_INPUT');
 for(const grant_revision of [-1,0.5,'1',Number.MAX_SAFE_INTEGER+1])expect(()=>taskToolInput('tasks_execute',{task_id:id,grant_revision,operation_id:'op'})).toThrow('INVALID_INPUT');
 expect(taskToolInput('tasks_execute',{task_id:id,grant_revision:0,operation_id:'op'}).grant_revision).toBe(0);
 expect(()=>taskToolInput('tasks_end',{task_id:id,grant_revision:1,reason:'restart'})).toThrow('INVALID_INPUT');
 expect(()=>taskToolInput('tasks_propose',{scope:'injected'})).toThrow('INVALID_INPUT');
 expect(()=>taskToolInput('tasks_capture',{task_id:id,change:null})).toThrow('INVALID_INPUT');
 expect(()=>taskToolInput('tasks_propose',{scope:{objective:'x'.repeat(65536)}})).toThrow('REQUEST_TOO_LARGE');
});
test('task execute cannot manufacture a proposal and consumes only the hook-bound immutable input',async()=>{
 const f=fixture(),input={scope};
 await expect(executeTaskTool('tasks_propose',input,f.context,f.client,f.approvals)).rejects.toThrow('TASK_APPROVAL_REQUIRED');expect(f.proposals()).toBe(0);
 const approval=await f.approvals.prepare('tasks_propose',input,f.context,f.client);
 await expect(f.approvals.prepare('tasks_propose',input,f.context,f.client)).rejects.toThrow('APPROVAL_PENDING');
 await expect(f.approvals.consume('tasks_propose',{scope:{...scope,objective:'different'}},f.context)).rejects.toThrow('TASK_APPROVAL_MISMATCH');
 const decision=approval.onResolution('allow-once');let finished=false;
 const pending=executeTaskTool('tasks_propose',input,f.context,f.client,f.approvals).then(result=>{finished=true;return result;});
 await Promise.resolve();expect(finished).toBe(false);f.ack();await decision;
 expect(await pending).toEqual({ok:true,data:{task_id:id,scope_digest:'a'.repeat(64),grant_revision:1}});
 await expect(f.approvals.consume('tasks_propose',input,f.context)).rejects.toThrow('TASK_APPROVAL_REQUIRED');expect(f.proposals()).toBe(1);
});
test('denied and revoked scope acknowledgements return the retained task ID for observation',async()=>{
 for(const revoked of [false,true]){
  const f=fixture(),input={scope};const approval=await f.approvals.prepare('tasks_propose',input,f.context,f.client);
  const decision=approval.onResolution(revoked?'allow-once':'deny');if(revoked)f.revoke();f.ack();await decision;
  expect(await f.approvals.consume('tasks_propose',input,f.context)).toEqual({ok:false,error:{code:revoked?'INVOCATION_EXPIRED':'APPROVAL_DENIED'},task_id:id});
 }
});
test('task resume binds the old task while ordinary members use one awaited client command without prompting',async()=>{
 const f=fixture();const approval=await f.approvals.prepare('tasks_resume',{task_id:id},f.context,f.client);const decision=approval.onResolution('allow-once');f.ack();await decision;
 expect(await executeTaskTool('tasks_resume',{task_id:id},f.context,f.client,f.approvals)).toMatchObject({ok:true,data:{task_id:id}});
 const change={kind:'document',operation_id:'op',document:'notes',tab_id:'t.0',action:'replace_text',fields:{find:'old',replacement:'new'}};
 for(const [name,input,expected] of [
  ['tasks_inspect',{task_id:id},{ok:true,data:{phase:'active'}}],
  ['tasks_capture',{task_id:id,change},{ok:true,data:{state:'previewed'}}],
  ['tasks_execute',{task_id:id,grant_revision:1,operation_id:'op'},{ok:true,data:{state:'uncertain'}}],
  ['tasks_discard',{task_id:id,operation_id:'op'},{ok:true,data:{state:'rejected'}}],
  ['tasks_end',{task_id:id,grant_revision:1,reason:'cancelled'},{ok:true,data:{phase:'closed'}}],
 ] as const)expect(await executeTaskTool(name,input,f.context,f.client,f.approvals)).toEqual(expected);
 expect(f.proposals()).toBe(0);expect(f.calls).toHaveLength(6);
});
test('an ended incarnation cannot lend its scope approval to a successor',async()=>{
 const f=fixture();await f.approvals.prepare('tasks_propose',{scope},f.context,f.client);
 await expect(f.approvals.consume('tasks_propose',{scope},{...f.context,sessionId:'successor'})).rejects.toThrow('TASK_APPROVAL_REQUIRED');
});
test('simultaneous hooks reserve before awaiting Core; capacity never evicts a live prompt',async()=>{
 const f=fixture();let release:((value:NativeTaskProposal)=>void)|undefined;
 const held:NativeTaskClient={...f.client,propose:()=>new Promise(done=>{release=done;})};
 const first=f.approvals.prepare('tasks_propose',{scope},f.context,held);
 await expect(f.approvals.prepare('tasks_propose',{scope},f.context,f.client)).rejects.toThrow('APPROVAL_PENDING');
 if(release===undefined)throw Error('Missing proposal resolver');release(f.proposal);await first;
 for(let index=1;index<64;index++)await f.approvals.prepare('tasks_propose',{scope},{...f.context,sessionId:String(index)},f.client);
 await expect(f.approvals.prepare('tasks_propose',{scope},{...f.context,sessionId:'overflow'},f.client)).rejects.toThrow('APPROVAL_CAPACITY');
 const decision=f.proposal.approval.onResolution('allow-once');f.ack();await decision;
 expect(await f.approvals.consume('tasks_propose',{scope},f.context)).toMatchObject({ok:true,data:{task_id:id}});
});
test('scope binding accepts reordered object keys without changing intent and bounds nested input',()=>{
 expect(taskToolInput('tasks_propose',{scope})).toEqual(taskToolInput('tasks_propose',{scope:{targets:scope.targets,max_operations:4,mapping_version:'v1',objective:'Correct note'}}));
 let deep:unknown='leaf';for(let i=0;i<18;i++)deep={child:deep};
 expect(()=>taskToolInput('tasks_propose',{scope:deep})).toThrow('INVALID_INPUT');
 for(const input of [undefined,null,[],{task_id:id,operation_id:''},{task_id:id,operation_id:'bad\n'}])expect(()=>taskToolInput('tasks_discard',input)).toThrow('INVALID_INPUT');
});
