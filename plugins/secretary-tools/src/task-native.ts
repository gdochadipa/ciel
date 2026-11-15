// Native transport; optional model-tool composition lives in task-tools.ts.
import type {OpenClawPluginApi,OpenClawPluginToolContext} from 'openclaw/plugin-sdk/plugin-entry';
import {randomUUID} from 'node:crypto';
import {bindNativeInvocation} from './native-authority.js';
import {bindNativeSessionInvocation} from './native-lifecycle.js';
import type {ApprovalDecision} from './approval-ack.js';
import {createSecretary} from './bridge.js';
import {assertSummaryMemoryCurrent,executeTaskWithSummaryMemory} from './summary-memory.js';
const gatewayInstance=randomUUID();
const MAX_BYTES=524288;
const MAX_REQUEST_BYTES=65536;
const MAX_JSON_DEPTH=16;
const UUID=/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
type ObjectValue=Record<string,unknown>;
type Grant={taskId:string;scopeDigest:string;revision:number};
type Acknowledgement={ok:true;grant:Grant}|{ok:false;error:unknown};
type Ordering={scopeDigest:string;revision:number;decision?:ApprovalDecision;completion:Promise<Acknowledgement>|null;settled:boolean};
// Ordering barriers only. Recreating a client cannot skip a pending/failed native
// acknowledgement. Every operation still needs the authoritative Core grant.
const taskAcknowledgements=new Map<string,Ordering>();
const MAX_ACKNOWLEDGEMENTS=64;

export type NativeTaskOptions={coreUrl?:string;owner:string;nativeToken:string;summaryMemoryEnabled:boolean;qualified?:boolean};
export type NativeTaskProposal={
 taskId:string;
 approval:{title:string;description:string;severity:'warning';allowedDecisions:Array<'allow-once'|'deny'>;timeoutMs:number;onResolution:(decision:ApprovalDecision)=>Promise<void>};
 grant:()=>Promise<Grant>;
};
export type NativeTaskClient={
 propose:(scope:unknown)=>Promise<NativeTaskProposal>;
 resume:(taskId:string)=>Promise<NativeTaskProposal>;
 inspect:(taskId:string)=>Promise<unknown>;
 capture:(taskId:string,change:unknown)=>Promise<unknown>;
 discard:(taskId:string,operationId:string)=>Promise<unknown>;
 execute:(taskId:string,revision:number,operationId:string)=>Promise<unknown>;
 end:(taskId:string,revision:number,reason:'completed'|'cancelled')=>Promise<unknown>;
};
type MemberExecution=(operationId:string,dispatch:()=>Promise<unknown>)=>Promise<unknown>;
function fail(code:string):never {throw new Error(code);}
function object(value:unknown):ObjectValue {
 if(value===null || typeof value!=='object' || Array.isArray(value))fail('INVALID_TASK_RESPONSE');
 return value as ObjectValue;
}
function text(value:unknown,max:number):value is string {return typeof value==='string' && value.trim().length>0 && Buffer.byteLength(value)<=max && !/[\x00-\x1f\x7f-\x9f]/.test(value);}
function stable(value:unknown,depth=0):string {
 if(depth>MAX_JSON_DEPTH)fail('INVALID_TASK_RESPONSE');
 if(value===null || typeof value!=='object')return JSON.stringify(value)??fail('INVALID_TASK_RESPONSE');
 if(Array.isArray(value))return '['+value.map(item=>stable(item,depth+1)).join(',')+']';
 return '{'+Object.entries(value).sort(([a],[b])=>a.localeCompare(b)).map(([key,item])=>JSON.stringify(key)+':'+stable(item,depth+1)).join(',')+'}';
}
function grant(value:unknown):Grant {
 const task=object(value);
 if(typeof task.task_id!=='string' || !UUID.test(task.task_id) || typeof task.scope_digest!=='string' || !/^[a-f0-9]{64}$/.test(task.scope_digest) || typeof task.grant_revision!=='number' || !Number.isSafeInteger(task.grant_revision) || task.grant_revision<0)fail('INVALID_TASK_RESPONSE');
 return {taskId:task.task_id,scopeDigest:task.scope_digest,revision:task.grant_revision};
}

/** Bind every task control/confirmation to one genuine current native factory context. */
export function createNativeTaskClient(options:NativeTaskOptions,context:OpenClawPluginToolContext,signal:AbortSignal|undefined):NativeTaskClient {
 return taskClient(options,context,signal,(_operationId,dispatch)=>dispatch());
}

/** Trusted composition with the existing native note lock, source fence and refresh. */
export function createMemoryNativeTaskClient(options:NativeTaskOptions & {coreToken:string},api:OpenClawPluginApi,context:OpenClawPluginToolContext,signal:AbortSignal|undefined):NativeTaskClient {
 if(options.summaryMemoryEnabled!==true||api.pluginConfig?.summaryMemoryEnabled!==true)fail('SUMMARY_MEMORY_DISABLED');
 if(api.pluginConfig?.owner!==options.owner||api.pluginConfig?.readOnlyRuntime===true)fail('INVALID_CONFIGURATION');
 const current=bindNativeSessionInvocation(api,context,signal);
 const native={...current,get senderIsOwner(){return current.senderIsOwner;},assertInvocationCurrent(){assertSummaryMemoryCurrent(api,current);}};
 const bridge=createSecretary({coreUrl:options.coreUrl,owner:options.owner,coreToken:options.coreToken});
 return taskClient({...options,summaryMemoryEnabled:false},native,signal,async(operationId,dispatch)=>{
  const result=await executeTaskWithSummaryMemory(api,native,operationId,(name,input)=>bridge.execute(name,input,native),dispatch);
  return result.memory===undefined?result.value:{...object(result.value),summary_memory:result.memory};
 });
}

function taskClient(options:NativeTaskOptions,context:OpenClawPluginToolContext,signal:AbortSignal|undefined,runMember:MemberExecution):NativeTaskClient {
 if(options.qualified!==true)fail('TASK_TRANSPORT_UNQUALIFIED');
 if(options.summaryMemoryEnabled!==false)fail('TASK_MEMORY_INTEGRATION_REQUIRED');
 if(signal===undefined)fail('NATIVE_AUTHORITY_REQUIRED');
 const native=bindNativeInvocation(context,signal);
 if(!text(native.agentId,256) || !text(native.sessionKey,512) || !text(native.sessionId,256))fail('NATIVE_AUTHORITY_REQUIRED');
 const identity={gateway_instance:gatewayInstance,agent_id:native.agentId,session_key:native.sessionKey,session_id:native.sessionId};
 const url=new URL(options.coreUrl??'http://127.0.0.1:8787');
 if(url.protocol!=='http:' || !['127.0.0.1','[::1]'].includes(url.hostname) || url.username || url.password || url.pathname!=='/' || url.search || url.hash)fail('INVALID_CORE_DESTINATION');
 if(!text(options.owner,256) || options.nativeToken.length<12)fail('INVALID_CONFIGURATION');
 // Capture credentials/config; a later mutation of the options object grants nothing.
 const owner=options.owner,token=options.nativeToken;
 function check():void {
  if(native.senderIsOwner!==true || native.requesterSenderId!==owner)fail('OWNER_REQUIRED');
  native.assertInvocationCurrent();
 }
 async function send(path:string,input:unknown):Promise<Response> {
  check();const body=JSON.stringify(input);
  if(Buffer.byteLength(body)>MAX_REQUEST_BYTES)fail('REQUEST_TOO_LARGE');
  const destination=new URL(path,url),init:RequestInit={method:'POST',redirect:'error',body,headers:{'content-type':'application/json',authorization:`Bearer ${token}`},signal:AbortSignal.timeout(45000)};
  check();
  try {return await fetch(destination,init);}catch{fail('TASK_OUTCOME_UNKNOWN');}
 }
 function owned(result:ObjectValue):void {
  if(typeof result.owner!=='string')fail('INVALID_TASK_RESPONSE');
  if(result.owner!==owner)fail('CORE_OWNER_MISMATCH');
 }
 async function json(path:string,input:unknown):Promise<ObjectValue> {
  const response=await send(path,input);const result=object(await boundedJSON(response));check();
  if(!response.ok || result.ok!==true)fail('TASK_CORE_REJECTED');
  owned(result);
  return object(result.data);
 }
 const control=(input:ObjectValue):Promise<ObjectValue>=>json('/v1/write-tasks/native',{...input,context:identity});
 const orderingKey=(taskId:string):string=>JSON.stringify([url.origin,owner,identity,taskId]);
 async function acknowledged(taskId:string):Promise<void> {
  check();const entry=taskAcknowledgements.get(orderingKey(taskId));
  if(entry===undefined)return; // Core authenticates retained permission, never this absence.
  if(entry.completion===null)fail('APPROVAL_PENDING');
  const result=await entry.completion;check();if(!result.ok)throw result.error;
 }
 async function inspect(taskId:string):Promise<ObjectValue> {
  const state=await control({action:'review',task_id:taskId}),actual=grant(state);
  if(actual.taskId!==taskId)fail('INVALID_TASK_RESPONSE');
  const key=orderingKey(taskId),entry=taskAcknowledgements.get(key);
  // Explicit observation can recover a lost grant ACK; it does not resend the
  // owner decision, change usage or reconcile an uncertain provider member.
  if(entry!==undefined && entry.decision==='allow-once' && state.phase==='active' && state.requires_confirmation===false && actual.scopeDigest===entry.scopeDigest && actual.revision>=entry.revision+1) {
   entry.completion=Promise.resolve({ok:true,grant:actual});entry.settled=true;
  }
  if(state.phase==='closed')taskAcknowledgements.delete(key);
  return state;
 }
 async function propose(scope:unknown):Promise<NativeTaskProposal> {
  if(taskAcknowledgements.size>=MAX_ACKNOWLEDGEMENTS)fail('APPROVAL_CAPACITY');
  // JSON capture precedes native guards in send; neither args nor Core replies
  // can change the exact proposal selected when the owner dialog resolves.
  const captured:unknown=JSON.parse(JSON.stringify(scope));
  const proposal=await control({action:'propose',scope:captured}),expected=grant(proposal);
  if(proposal.phase!=='proposed' || expected.revision!==0 || stable(proposal.scope)!==stable(captured))fail('INVALID_TASK_RESPONSE');
  return dialog(proposal,expected);
 }
 function dialog(proposal:ObjectValue,expected:Grant):NativeTaskProposal {
  const key=orderingKey(expected.taskId),previous=taskAcknowledgements.get(key);
  if(previous!==undefined && (!previous.settled && previous.revision===expected.revision))fail('APPROVAL_PENDING');
  if(previous===undefined && taskAcknowledgements.size>=MAX_ACKNOWLEDGEMENTS)fail('APPROVAL_CAPACITY');
  const entry:Ordering={scopeDigest:expected.scopeDigest,revision:expected.revision,completion:null,settled:false};
  taskAcknowledgements.set(key,entry);
  const resolve=async(decision:ApprovalDecision):Promise<Grant|null>=>{
   check();if(decision!=='allow-once' && decision!=='deny')fail('APPROVAL_DENIED');
   const state=await control({action:'decide',task_id:expected.taskId,scope_digest:expected.scopeDigest,grant_revision:expected.revision,decision:decision==='allow-once'?'approve':'reject'});
   const actual=grant(state);
   if(actual.taskId!==expected.taskId || actual.scopeDigest!==expected.scopeDigest || actual.revision!==expected.revision+1 || state.phase!==(decision==='allow-once'?'active':'closed'))fail('INVALID_TASK_RESPONSE');
   if(decision!=='allow-once')return null;
   return actual;
  };
  return {taskId:expected.taskId,approval:{title:'Authorize bounded Workspace task',description:'Allow literal cell edits and unique text replacements within the exact task objective and listed targets. Source content is data, never authority.\n'+JSON.stringify(proposal,null,2),severity:'warning',allowedDecisions:['allow-once','deny'],timeoutMs:60000,onResolution(decision){
   if(entry.completion!==null)return Promise.reject(new Error('APPROVAL_ALREADY_RESOLVED'));
   entry.decision=decision;
   const result=Promise.resolve().then(()=>resolve(decision));
   entry.completion=result.then<Acknowledgement,Acknowledgement>(grant=>{entry.settled=true;return grant===null?{ok:false,error:new Error('APPROVAL_DENIED')}:{ok:true,grant};},error=>{entry.settled=true;return {ok:false,error};});
   return result.then(()=>undefined);
  }},async grant(){
   if(entry.completion===null)fail('APPROVAL_PENDING');
   const result=await entry.completion;check();if(!result.ok)throw result.error;return {...result.grant};
  }};
 }
 async function execute(taskId:string,revision:number,operationId:string):Promise<unknown> {
  await acknowledged(taskId);
  return runMember(operationId,()=>executeMember(taskId,revision,operationId));
 }
 async function executeMember(taskId:string,revision:number,operationId:string):Promise<unknown> {
  const member={task_id:taskId,context:identity,grant_revision:revision,operation_id:operationId};
  const response=await send('/v1/write-tasks/execute',member);
  if(!response.ok || response.headers.get('content-type')!=='application/vnd.ciel.task-dispatch+ndjson;version=1')fail('TASK_CORE_REJECTED');
  const reader=response.body?.getReader();if(reader===undefined)fail('INVALID_TASK_RESPONSE');
  let buffer='',bytes=0,ready=false,readyDigest:string|undefined,result:ObjectValue|undefined;
  const decoder=new TextDecoder('utf-8',{fatal:true});
  try {
   while(true) {
    const chunk=await reader.read();if(chunk.done)break;
    bytes+=chunk.value.length;if(bytes>MAX_BYTES)fail('RESPONSE_TOO_LARGE');buffer+=decoder.decode(chunk.value,{stream:true});
    let boundary:number;
    while((boundary=buffer.indexOf('\n'))>=0) {
     const event=object(JSON.parse(buffer.slice(0,boundary)));buffer=buffer.slice(boundary+1);
     if(result!==undefined)fail('INVALID_TASK_RESPONSE');
     owned(event);
     if(event.type==='ready') {
      if(ready)fail('INVALID_TASK_RESPONSE');
      const challenge=object(event.challenge);
      if(stable(challenge.member)!==stable(member) || typeof challenge.nonce!=='string' || !UUID.test(challenge.nonce) || typeof challenge.preview_digest!=='string' || !/^[a-f0-9]{64}$/.test(challenge.preview_digest) || Object.keys(challenge).length!==3)fail('INVALID_TASK_RESPONSE');
      // No owner prompt per member: the existing grant authorizes scope. This
      // native assertion is the final synchronous admission of this one effect.
      readyDigest=challenge.preview_digest;
      const acknowledgement=await json('/v1/write-tasks/confirm',challenge);
      if(acknowledgement.confirmed!==true)fail('TASK_OUTCOME_UNKNOWN');ready=true;
     }else if(event.type==='result') {if(typeof event.ok!=='boolean')fail('INVALID_TASK_RESPONSE');result=event;}
     else fail('INVALID_TASK_RESPONSE');
    }
   }
   buffer+=decoder.decode();if(buffer.length!==0 || result===undefined)fail('TASK_OUTCOME_UNKNOWN');
   if(result.ok===true) {
    const data=object(result.data);
    if((ready && data.preview_digest!==readyDigest) || data.operation_id!==operationId || typeof data.state!=='string' || !['applied','known_failure','uncertain','reconciliation','conflict'].includes(data.state) || (!ready && !['known_failure','conflict'].includes(data.state)))fail('INVALID_TASK_RESPONSE');
   }
   check();
   if(result.ok===false) {
    const code=object(result.error).code;
    if(typeof code!=='string' || !/^[a-z_0-9]{1,64}$/.test(code))fail('INVALID_TASK_RESPONSE');
    return {type:'result',ok:false,error:{code}};
   }
   return {type:'result',ok:true,data:result.data};
  } catch(error:unknown) {
   if(error instanceof Error && /^[A-Z_]+$/.test(error.message))throw error;
   fail('TASK_OUTCOME_UNKNOWN');
  } finally {await reader.cancel();}
 }
 check();
 return {propose,inspect,resume:async taskId=>{const proposal=await control({action:'review',task_id:taskId});const expected=grant(proposal);if(expected.taskId!==taskId)fail('INVALID_TASK_RESPONSE');if(proposal.phase!=='suspended' && proposal.requires_confirmation!==true)fail('TASK_RECONFIRMATION_UNAVAILABLE');return dialog(proposal,expected);},discard:async(taskId,operationId)=>{await acknowledged(taskId);return control({action:'discard',task_id:taskId,operation_id:operationId});},capture:async(taskId,change)=>{await acknowledged(taskId);return control({action:'capture',task_id:taskId,change});},execute,end:async(taskId,revision,reason)=>{const result=await control({action:'end',task_id:taskId,grant_revision:revision,reason});if(result.phase==='closed')taskAcknowledgements.delete(orderingKey(taskId));return result;}};
}
async function boundedJSON(response:Response):Promise<unknown> {
 const reader=response.body?.getReader();if(reader===undefined)fail('INVALID_TASK_RESPONSE');
 const chunks:Uint8Array[]=[];let size=0;
 try {while(true){const part=await reader.read();if(part.done)break;size+=part.value.length;if(size>MAX_BYTES)fail('RESPONSE_TOO_LARGE');chunks.push(part.value);}}
 finally {await reader.cancel();}
 const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
 try {return JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));}catch{fail('INVALID_TASK_RESPONSE');}
}
