import {test,expect,spyOn} from 'bun:test';
import {ApprovalAcknowledgements} from '../src/approval-ack';
import {createSecretary,type Identity} from '../src/bridge';

function deferred<T>():{promise:Promise<T>;resolve:(value:T)=>void} {
 let complete:((value:T)=>void)|undefined;
 const promise=new Promise<T>(resolve=>{complete=resolve;});
 return {promise,resolve(value){if(complete===undefined)throw Error('invariant: missing resolver');complete(value);}};
}
function observed<T>(promise:Promise<T>):Promise<{ok:true;value:T}|{ok:false;error:unknown}> {
 return promise.then(value=>({ok:true,value}),(error:unknown)=>({ok:false,error}));
}
function code(result:{ok:boolean;error?:unknown}):string {
 return result.error instanceof Error?result.error.message:'unexpected successful result';
}
const check=():void=>{ /* This unit context has not been revoked. */ };

test('barrier remains pending until its one Core acknowledgement succeeds',async()=>{
 const registry=new ApprovalAcknowledgements();const response=deferred<void>();const started=deferred<void>();
 const approval=registry.reserve('op',Date.now()+60000,check,async()=>{started.resolve(undefined);await response.promise;});
 await expect(registry.wait('op',check)).rejects.toThrow('APPROVAL_PENDING');
 const callback=approval.onResolution('allow-once');const ready=observed(registry.wait('op',check));await started.promise;
 await expect(approval.onResolution('allow-once')).rejects.toThrow('APPROVAL_ALREADY_RESOLVED');
 response.resolve(undefined);await callback;expect((await ready).ok).toBe(true);
});

test('failed and denied Core decisions remain blocking rather than becoming grants',async()=>{
 const registry=new ApprovalAcknowledgements();let sends=0;
 const rejected=registry.reserve('rejected',Date.now()+60000,check,async()=>{sends++;throw Error('APPROVAL_REJECTED');});
 expect(code(await observed(rejected.onResolution('allow-once')))).toBe('APPROVAL_REJECTED');
 await expect(registry.wait('rejected',check)).rejects.toThrow('APPROVAL_REJECTED');
 const denied=registry.reserve('denied',Date.now()+60000,check,async decision=>{sends++;expect(decision).toBe('deny');});
 await denied.onResolution('deny');await expect(registry.wait('denied',check)).rejects.toThrow('APPROVAL_DENIED');
 for(const decision of ['timeout','cancelled','allow-always'] as const) {
  const gate=registry.reserve(decision,Date.now()+60000,check,async()=>{sends++;});
  await expect(gate.onResolution(decision)).rejects.toThrow('APPROVAL_DENIED');
  await expect(registry.wait(decision,check)).rejects.toThrow('APPROVAL_DENIED');
 }
 expect(sends).toBe(2);
});

test('lost acknowledgement has a bounded sticky unknown outcome even after a late success',async()=>{
 const registry=new ApprovalAcknowledgements(10);const response=deferred<void>();
 const gate=registry.reserve('op',Date.now()+60000,check,async()=>response.promise);
 const callback=gate.onResolution('allow-once');
 await expect(registry.wait('op',check)).rejects.toThrow('APPROVAL_ACK_OUTCOME_UNKNOWN');
 response.resolve(undefined);await callback;
 await expect(registry.wait('op',check)).rejects.toThrow('APPROVAL_ACK_OUTCOME_UNKNOWN');
});

test('currentness is checked again after waiting and failed consumption never sends',async()=>{
 const registry=new ApprovalAcknowledgements();let current=true;let sends=0;const response=deferred<void>();const started=deferred<void>();
 const assertCurrent=():void=>{if(!current)throw Error('INVOCATION_EXPIRED');};
 const gate=registry.reserve('op',Date.now()+60000,check,async()=>{started.resolve(undefined);await response.promise;});
 const callback=gate.onResolution('allow-once');const waiting=observed(registry.wait('op',assertCurrent));await started.promise;current=false;
 response.resolve(undefined);await callback;expect(code(await waiting)).toBe('INVOCATION_EXPIRED');
 const broken=registry.reserve('broken',Date.now()+60000,check,async()=>{sends++;},()=>{throw Error('CONSUMPTION_FAILED');});
 await expect(broken.onResolution('allow-once')).rejects.toThrow('CONSUMPTION_FAILED');
 await expect(registry.wait('broken',check)).rejects.toThrow('CONSUMPTION_FAILED');expect(sends).toBe(0);
});

test('pending barriers have a fixed capacity and duplicate callbacks do not consume a new preview',async()=>{
 const registry=new ApprovalAcknowledgements();let consumed=0;
 const gate=registry.reserve('first',Date.now()+60000,check,async()=>{},()=>{consumed++;});
 expect(()=>registry.reserve('first',Date.now()+60000,check,async()=>{})).toThrow('APPROVAL_PENDING');
 for(let i=1;i<64;i++)registry.reserve(`op-${i}`,Date.now()+60000,check,async()=>{});
 expect(()=>registry.reserve('overflow',Date.now()+60000,check,async()=>{})).toThrow('APPROVAL_CAPACITY');
 await gate.onResolution('allow-once');await registry.wait('first',check);
 registry.reserve('overflow',Date.now()+60000,check,async()=>{});
 await expect(gate.onResolution('allow-once')).rejects.toThrow('APPROVAL_ALREADY_RESOLVED');expect(consumed).toBe(1);
});

test('expired captures and invalid local acknowledgement deadlines fail closed',async()=>{
 for(const timeout of [0,-1,1.5,NaN,Infinity,11001])expect(()=>new ApprovalAcknowledgements(timeout)).toThrow('INVALID_APPROVAL_TIMEOUT');
 const registry=new ApprovalAcknowledgements();
 for(const expiry of [0,Date.now()-1,NaN,Infinity])expect(()=>registry.reserve('expired',expiry,check,async()=>{})).toThrow('PREVIEW_EXPIRED');
});

const owner:Identity={requesterSenderId:'owner',senderIsOwner:true};
const options={owner:'owner',coreToken:'synthetic-core',approvalToken:'synthetic-approval',approvalTransportQualified:true};
const change={operation_id:'op',spreadsheet:'sheet',view:'grid',action:'update',fields:{range:'A1:A1',values:[['new']]}};
const preview={...change,resource_kind:'sheets_grid',actor:'owner',record_id:'physical:7',before:[[{stringValue:'old'}]],after:[[{stringValue:'new'}]],mapping_version:'v1',source_revision:'r1',preview_digest:'captured-digest',expires_at:'2099-01-01T00:00:00Z',state:'previewed'};

async function fixture(identity:Identity=owner) {
 const response=deferred<Response>();const started=deferred<void>();const paths:string[]=[];
 const transport=spyOn(globalThis,'fetch').mockImplementation(async(input,init)=>{
  const request=new Request(input,init);const path=new URL(request.url).pathname;paths.push(path);
  if(path.endsWith('/sheets_preview_change'))return Response.json({ok:true,data:preview});
  if(path.endsWith('/approval')){started.resolve(undefined);return response.promise;}
  if(path.endsWith('/sheets_update'))return Response.json({ok:true,data:{state:'applied'}});
  throw Error('invariant: unexpected test route');
 });
 try {
  const bridge=createSecretary(options);await bridge.execute('sheets_preview_change',change,identity);
  const approval=await bridge.prepareApproval('sheets_update',{operation_id:'op'},identity);
  if(approval===undefined)throw Error('invariant: required approval missing');
  return {bridge,approval,paths,response,started,close:()=>transport.mockRestore()};
 } catch(error:unknown){transport.mockRestore();throw error;}
}

test('bridge waits for Core approval acknowledgement when the native caller does not await its callback',async()=>{
 const f=await fixture();try {
  const callback=observed(f.approval.onResolution('allow-once'));
  const write=observed(f.bridge.execute('sheets_update',{operation_id:'op'},owner));await f.started.promise;
  expect(f.paths).toEqual(['/v1/tools/sheets_preview_change','/v1/operations/op/approval']);
  f.response.resolve(Response.json({ok:true,data:{state:'approved'}}));expect((await callback).ok).toBe(true);expect((await write).ok).toBe(true);
  expect(f.paths).toEqual(['/v1/tools/sheets_preview_change','/v1/operations/op/approval','/v1/tools/sheets_update']);
 } finally {f.close();}
});

test('bridge never commits after an asynchronously refused or malformed approval acknowledgement',async()=>{
 for(const result of [{ok:false,error:{code:'operation_conflict'}},{ok:true,data:{state:'previewed'}},{ok:true,data:{state:'rejected'}},{ok:true,data:null}]) {
  const f=await fixture();try {
   const callback=observed(f.approval.onResolution('allow-once'));const write=observed(f.bridge.execute('sheets_update',{operation_id:'op'},owner));await f.started.promise;
   f.response.resolve(Response.json(result));expect(code(await callback)).toBe('APPROVAL_REJECTED');expect(code(await write)).toBe('APPROVAL_REJECTED');
   expect(f.paths).toEqual(['/v1/tools/sheets_preview_change','/v1/operations/op/approval']);
  } finally {f.close();}
 }
});

test('bridge rechecks invocation authority after a delayed acknowledgement',async()=>{
 let current=true;const identity:Identity={...owner,assertInvocationCurrent(){if(!current)throw Error('INVOCATION_EXPIRED');}};
 const f=await fixture(identity);try {
  const callback=observed(f.approval.onResolution('allow-once'));const write=observed(f.bridge.execute('sheets_update',{operation_id:'op'},identity));await f.started.promise;
  current=false;f.response.resolve(Response.json({ok:true,data:{state:'approved'}}));
  expect(code(await callback)).toBe('INVOCATION_EXPIRED');expect(code(await write)).toBe('INVOCATION_EXPIRED');expect(f.paths).toHaveLength(2);
 } finally {f.close();}
});


test('commit preparation captures the operation before awaiting Core and runs no caller effects on refusal',async()=>{
 const f=await fixture();try {
  const callback=observed(f.approval.onResolution('allow-once'));const input={operation_id:'op'};
  const prepared=f.bridge.prepareCommit('sheets_update',input,owner);await f.started.promise;
  input.operation_id='substituted';f.response.resolve(Response.json({ok:true,data:{state:'approved'}}));
  expect(await prepared).toEqual({operation_id:'op'});expect((await callback).ok).toBe(true);
  expect(f.paths.length).toBe(2);
 } finally {f.close();}
 const refused=await fixture();try {
  const callback=observed(refused.approval.onResolution('allow-once'));let effects=0;
  const prepared=observed(refused.bridge.prepareCommit('sheets_update',{operation_id:'op'},owner).then(()=>{effects++;}));
  await refused.started.promise;refused.response.resolve(Response.json({ok:false,error:{code:'operation_conflict'}},{status:409}));
  expect((await callback).ok).toBe(false);expect(code(await prepared)).toBe('APPROVAL_REJECTED');
  expect(effects).toBe(0);expect(refused.paths.length).toBe(2);
 } finally {refused.close();}
});

test('commit preparation rejects unauthorized profiles, non-commit tools and malformed identifiers',async()=>{
 const bridge=createSecretary(options);
 await expect(bridge.prepareCommit('sheets_update',{operation_id:'op'},{})).rejects.toThrow('OWNER_REQUIRED');
 await expect(bridge.prepareCommit('sheets_read',{operation_id:'op'},owner)).rejects.toThrow('UNKNOWN_TOOL');
 for(const input of [null,{}, {operation_id:0},{operation_id:''},{operation_id:'../op'},{operation_id:'a'.repeat(101)},{operation_id:'op',extra:true}])
  await expect(bridge.prepareCommit('sheets_update',input,owner)).rejects.toThrow('INVALID_INPUT');
 const readOnly=createSecretary({owner:'owner',coreToken:'synthetic-core',readOnlyRuntime:true});
 await expect(readOnly.prepareCommit('docs_apply',{operation_id:'op'},owner)).rejects.toThrow('READONLY_PROFILE');
 const disabled=createSecretary({owner:'owner',coreToken:'synthetic-core'});
 await expect(disabled.prepareCommit('docs_apply',{operation_id:'op'},owner)).rejects.toThrow('WRITES_DISABLED');
 expect(await bridge.prepareCommit('docs_apply',{operation_id:'a'.repeat(100)},owner)).toEqual({operation_id:'a'.repeat(100)});
});

test('approval callback retains its captured operation even if caller parameters are later mutated',async()=>{
 const f=await fixture();try {
  const input={operation_id:'op'};
  // The existing unresolved approval prevents a second reservation for the same ID.
  // A separate bridge receives the same immutable Core preview for this case.
  const bridge=createSecretary(options);await bridge.execute('sheets_preview_change',change,owner);
  const approval=await bridge.prepareApproval('sheets_update',input,owner);
  if(approval===undefined)throw Error('invariant: required approval missing');
  input.operation_id='substituted';const callback=observed(approval.onResolution('allow-once'));
  await f.started.promise;expect(f.paths.at(-1)).toBe('/v1/operations/op/approval');
  f.response.resolve(Response.json({ok:true,data:{state:'approved'}}));expect((await callback).ok).toBe(true);
 } finally {f.close();}
});


test('original expiry blocks delayed decisions and acknowledgements and permits bounded entry cleanup',async()=>{
 let now=Date.now();const clock=spyOn(Date,'now').mockImplementation(()=>now);
 try {
  const registry=new ApprovalAcknowledgements();let sends=0;
  const delayed=registry.reserve('delayed',now+1000,check,async()=>{sends++;});
  now+=1000;await expect(delayed.onResolution('allow-once')).rejects.toThrow('PREVIEW_EXPIRED');
  await expect(registry.wait('delayed',check)).rejects.toThrow('PREVIEW_EXPIRED');expect(sends).toBe(0);
  const response=deferred<void>(),started=deferred<void>();
  const gate=registry.reserve('waiting',now+1000,check,async()=>{started.resolve(undefined);await response.promise;});
  const callback=gate.onResolution('allow-once'),waiting=observed(registry.wait('waiting',check));
  await started.promise;now+=1000;response.resolve(undefined);await callback;
  expect(code(await waiting)).toBe('PREVIEW_EXPIRED');
  for(let i=0;i<64;i++)registry.reserve(`expired-${i}`,now+1000,check,async()=>{});
  now+=1000;const fresh=registry.reserve('fresh',now+1000,check,async()=>{});
  await fresh.onResolution('allow-once');await registry.wait('fresh',check);
 } finally {clock.mockRestore();}
});
