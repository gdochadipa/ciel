import type {OpenClawPluginApi,OpenClawPluginToolContext} from 'openclaw/plugin-sdk/plugin-entry';
import {registerNativeSessionLifecycle,type NativeSessionIdentity} from './native-lifecycle.js';
import {captureNativeTaskRevoker,type NativeTaskClient,type NativeTaskOptions,type NativeTaskRevoker} from './task-native.js';
const MAX_ENROLLED_CONTEXTS=64;
type Entry={key:string;captured:NativeTaskRevoker;ended:boolean;failed:boolean;work:Promise<boolean>|null};
function sameSession(a:NativeSessionIdentity,b:NativeSessionIdentity):boolean {return a.agentId===b.agentId&&a.sessionKey===b.sessionKey&&a.sessionId===b.sessionId;}
function fail():never {throw new Error('TASK_LIFECYCLE_RECONCILIATION_REQUIRED');}
/** Ordering/transport only. Exact ended-context receipts and grants live in Core. */
export class TaskSessionRetirements {
 private readonly entries=new Map<string,Entry>();
 enroll(captured:NativeTaskRevoker):void {
  captured.check();const key=JSON.stringify([captured.connection,captured.identity]);
  if(this.entries.has(key))return;
  if(this.entries.size>=MAX_ENROLLED_CONTEXTS)throw new Error('TASK_LIFECYCLE_CAPACITY');
  this.entries.set(key,{key,captured,ended:false,failed:false,work:null});
 }
 private start(entry:Entry,retry:boolean):Promise<boolean> {
  if(entry.work===null||(retry&&entry.failed)){
   entry.failed=false;
   // Capture failures as an observed outcome: detached SDK hooks cannot make a
   // failed ACK look successful. The next admitted invocation may retry this
   // idempotent metadata revocation, never a provider write or owner decision.
   entry.work=Promise.resolve().then(()=>entry.captured.revoke()).then(()=>{this.entries.delete(entry.key);return true;},()=>{entry.failed=true;return false;});
  }
  return entry.work;
 }
 async end(identity:NativeSessionIdentity):Promise<void> {
  const matching=[...this.entries.values()].filter(entry=>sameSession(entry.captured.identity,identity));
  for(const entry of matching)entry.ended=true;
  if((await Promise.all(matching.map(entry=>this.start(entry,false)))).some(ok=>!ok))fail();
 }
 async wait(captured:NativeTaskRevoker):Promise<void> {
  const matching=[...this.entries.values()].filter(entry=>entry.ended&&entry.captured.connection===captured.connection&&entry.captured.identity.agentId===captured.identity.agentId&&entry.captured.identity.sessionKey===captured.identity.sessionKey);
  if((await Promise.all(matching.map(entry=>this.start(entry,true)))).some(ok=>!ok))fail();
  captured.check();
 }
}
const retirements=new WeakMap<OpenClawPluginApi,TaskSessionRetirements>();
function state(api:OpenClawPluginApi):TaskSessionRetirements {
 let value=retirements.get(api);if(value===undefined){value=new TaskSessionRetirements();retirements.set(api,value);}return value;
}
/** Registration does not read secrets or access Core, including in read-only mode. */
export function registerNativeTaskLifecycle(api:OpenClawPluginApi):void {
 const pending=state(api);registerNativeSessionLifecycle(api,identity=>pending.end(identity));
}
/** Only live qualified task invocations enroll; ordinary run completion keeps the grant. */
export function withNativeTaskLifecycle(api:OpenClawPluginApi,options:NativeTaskOptions,context:OpenClawPluginToolContext,client:NativeTaskClient):NativeTaskClient {
 const pending=state(api),captured=captureNativeTaskRevoker(options,context);
 async function invoke<T>(work:()=>Promise<T>):Promise<T> {await pending.wait(captured);pending.enroll(captured);return work();}
 return {
  propose:scope=>invoke(()=>client.propose(scope)),resume:id=>invoke(()=>client.resume(id)),inspect:id=>invoke(()=>client.inspect(id)),
  capture:(id,change)=>invoke(()=>client.capture(id,change)),discard:(id,operation)=>invoke(()=>client.discard(id,operation)),
  execute:(id,revision,operation)=>invoke(()=>client.execute(id,revision,operation)),end:(id,revision,reason)=>invoke(()=>client.end(id,revision,reason)),
 };
}
