// Model arguments select intent only; the native hook owns scope approval.
import type {OpenClawPluginApi,OpenClawPluginToolContext} from 'openclaw/plugin-sdk/plugin-entry';
import {createNativeTaskClient,createMemoryNativeTaskClient,type NativeTaskClient,type NativeTaskProposal} from './task-native.js';

import {isTaskApprovalTool,type TaskToolName} from './task-tool-specs.js';
function fail(code:string):never {throw new Error(code);}
/** Explicit opt-in only; credentials are captured from trusted process configuration. */
export function taskToolClient(api:OpenClawPluginApi,context:OpenClawPluginToolContext,signal:AbortSignal|undefined):NativeTaskClient {
 const config=api.pluginConfig??{};
 if(config.readOnlyRuntime===true)fail('READONLY_TASKS_DISABLED');
 if(config.taskToolsEnabled!==true)fail('TASK_TOOLS_DISABLED');
 if(config.taskTransportQualified!==true||config.approvalTransportQualified!==true)fail('TASK_TRANSPORT_UNQUALIFIED');
 const coreToken=process.env.SECRETARY_CORE_TOKEN??'',nativeToken=process.env.SECRETARY_APPROVAL_TOKEN??'';
 if(coreToken.length===0||nativeToken===coreToken)fail('INVALID_CONFIGURATION');
 const options={coreUrl:typeof config.coreUrl==='string'?config.coreUrl:undefined,owner:typeof config.owner==='string'?config.owner:'',nativeToken,summaryMemoryEnabled:config.summaryMemoryEnabled===true,qualified:true};
 return options.summaryMemoryEnabled?createMemoryNativeTaskClient({...options,coreToken},api,context,signal):createNativeTaskClient(options,context,signal);
}
type Pending={input:string;proposal:NativeTaskProposal|null;current:()=>void};
const MAX_PENDING=64;
/** Ordering state only: durable authority and usage remain in the Core journal. */
export class TaskToolApprovals {
 private pending=new Map<string,Pending>();
 private key(name:TaskToolName,context:OpenClawPluginToolContext):string {return JSON.stringify([name,context.agentId,context.sessionKey,context.sessionId]);}
 async prepare(name:TaskToolName,input:Record<string,unknown>,context:OpenClawPluginToolContext,client:NativeTaskClient):Promise<NativeTaskProposal['approval']> {
  if(!isTaskApprovalTool(name))fail('INVALID_INPUT');
  for(const [key,entry] of this.pending){try{entry.current();}catch{this.pending.delete(key);}}
  const key=this.key(name,context);if(this.pending.has(key))fail('APPROVAL_PENDING');
  if(this.pending.size>=MAX_PENDING)fail('APPROVAL_CAPACITY');
  const encoded=JSON.stringify(input);
  // Reserve before source/HTTP awaits so concurrent hook invocations cannot mint
  // two proposals. The temporary entry never supplies write authority.
  const assertion=context.assertInvocationCurrent;if(typeof assertion!=='function')fail('NATIVE_AUTHORITY_REQUIRED');
  const current=()=>assertion();
  const placeholder:Pending={input:encoded,proposal:null,current};
  this.pending.set(key,placeholder);
  try {
   const proposal=name==='tasks_propose'?await client.propose(input.scope):await client.resume(String(input.task_id));
   current();if(this.pending.get(key)!==placeholder)fail('INVOCATION_EXPIRED');
   this.pending.set(key,{input:encoded,proposal,current});return proposal.approval;
  }catch(error:unknown){if(this.pending.get(key)===placeholder)this.pending.delete(key);throw error;}
 }
 async consume(name:TaskToolName,input:Record<string,unknown>,context:OpenClawPluginToolContext):Promise<unknown> {
  const key=this.key(name,context),entry=this.pending.get(key);
  if(entry===undefined||entry.proposal===null)fail('TASK_APPROVAL_REQUIRED');
  if(entry.input!==JSON.stringify(input))fail('TASK_APPROVAL_MISMATCH');
  this.pending.delete(key);
  try {const grant=await entry.proposal.grant();entry.current();return {ok:true,data:{task_id:grant.taskId,scope_digest:grant.scopeDigest,grant_revision:grant.revision}};}
  catch(error:unknown){const code=error instanceof Error&&/^[A-Z_]+$/.test(error.message)?error.message:'TASK_OUTCOME_UNKNOWN';return {ok:false,error:{code},task_id:entry.proposal.taskId};}
 }
}
/** One awaited command, never a batch executor or provider retry loop. */
export async function executeTaskTool(name:TaskToolName,input:Record<string,unknown>,context:OpenClawPluginToolContext,client:NativeTaskClient,approvals:TaskToolApprovals):Promise<unknown> {
 switch(name){
  case 'tasks_propose':case 'tasks_resume':return approvals.consume(name,input,context);
  case 'tasks_inspect':return {ok:true,data:await client.inspect(String(input.task_id))};
  case 'tasks_capture':return {ok:true,data:await client.capture(String(input.task_id),input.change)};
  case 'tasks_discard':return {ok:true,data:await client.discard(String(input.task_id),String(input.operation_id))};
  case 'tasks_execute':return client.execute(String(input.task_id),Number(input.grant_revision),String(input.operation_id));
  case 'tasks_end':{
   if(input.reason!=='completed'&&input.reason!=='cancelled')fail('INVALID_INPUT');
   return {ok:true,data:await client.end(String(input.task_id),Number(input.grant_revision),input.reason)};
  }
  default:{const unhandled:never=name;throw new Error(`Unknown task tool: ${unhandled}`);}
 }
}
