// Ordering barriers, not write grants: Core remains authoritative for every edit.
export type ApprovalDecision = 'allow-once' | 'allow-always' | 'deny' | 'timeout' | 'cancelled';
type Outcome = {ok:true;allowed:boolean} | {ok:false;error:unknown};
type Entry = {
 expiresAt:number;
 outcome:Promise<Outcome>|null;
 settled:boolean;
 blocked:Error|null;
};
export type ApprovalResolution = {
 onResolution(decision:ApprovalDecision):Promise<void>;
};
const MAX_ENTRIES=64;
const MAX_ACK_WAIT_MS=11000;
const failure=(code:string):Error=>new Error(code);

/** Await Core acknowledgement even when the native host only notifies resolution. */
export class ApprovalAcknowledgements {
 private readonly entries=new Map<string,Entry>();
 constructor(private readonly waitMs=MAX_ACK_WAIT_MS) {
  if(!Number.isSafeInteger(waitMs) || waitMs<1 || waitMs>MAX_ACK_WAIT_MS)throw failure('INVALID_APPROVAL_TIMEOUT');
 }

 reserve(id:string,expiresAt:number,check:()=>void,send:(decision:'allow-once'|'deny')=>Promise<void>,consume?:()=>void):ApprovalResolution {
  if(!Number.isFinite(expiresAt) || expiresAt<=Date.now())throw failure('PREVIEW_EXPIRED');
  for(const [key,entry] of this.entries)if(entry.expiresAt<=Date.now())this.entries.delete(key);
  const previous=this.entries.get(id);
  if(previous!==undefined && !previous.settled)throw failure('APPROVAL_PENDING');
  if(previous===undefined && this.entries.size>=MAX_ENTRIES)throw failure('APPROVAL_CAPACITY');
  const entry:Entry={expiresAt,outcome:null,settled:false,blocked:null};
  this.entries.set(id,entry);
  return {onResolution:(decision)=>{
   if(entry.outcome!==null)return Promise.reject(failure('APPROVAL_ALREADY_RESOLVED'));
   // Register the outcome synchronously before the SDK can begin tool execution.
   // Both rejection observers preserve a failure; no rejected promise is ignored.
   const completion=Promise.resolve().then(async()=>{
    consume?.();
    check();
    if(entry.expiresAt<=Date.now())throw failure('PREVIEW_EXPIRED');
    if(decision!=='allow-once' && decision!=='deny')throw failure('APPROVAL_DENIED');
    await send(decision);
    check();
   });
   entry.outcome=completion.then<Outcome,Outcome>(
    ()=>{entry.settled=true;return {ok:true,allowed:decision==='allow-once'};},
    (error:unknown)=>{entry.settled=true;return {ok:false,error};},
   );
   return completion;
  }};
 }

 async wait(id:string,check:()=>void):Promise<void> {
  const entry=this.entries.get(id);
  if(entry===undefined)return; // Absence grants nothing; Core must still approve.
  check();
  if(entry.expiresAt<=Date.now())throw failure('PREVIEW_EXPIRED');
  if(entry.blocked!==null)throw entry.blocked;
  if(entry.outcome===null)throw failure('APPROVAL_PENDING');
  const outcome=await this.bounded(entry,entry.outcome);
  check();
  if(entry.expiresAt<=Date.now())throw failure('PREVIEW_EXPIRED');
  if(entry.blocked!==null)throw entry.blocked;
  if(!outcome.ok)throw outcome.error;
  if(!outcome.allowed)throw failure('APPROVAL_DENIED');
  if(this.entries.get(id)===entry)this.entries.delete(id);
 }

 private async bounded(entry:Entry,outcome:Promise<Outcome>):Promise<Outcome> {
  let timer:ReturnType<typeof setTimeout>|undefined;
  try {
   return await Promise.race([outcome,new Promise<Outcome>(resolve=>{
    timer=setTimeout(()=>{
     const code=entry.expiresAt<=Date.now()?'PREVIEW_EXPIRED':'APPROVAL_ACK_OUTCOME_UNKNOWN';
     const error=entry.blocked??failure(code);entry.blocked=error;
     resolve({ok:false,error});
    },Math.min(this.waitMs,entry.expiresAt-Date.now()));
   })]);
  } finally {if(timer!==undefined)clearTimeout(timer);}
 }
}
