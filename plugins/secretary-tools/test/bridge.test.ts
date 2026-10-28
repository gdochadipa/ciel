import { test, expect } from 'bun:test';
import { createSecretary,secretaryToolNames } from '../src/bridge';
const owner = { requesterSenderId: '+628123', senderIsOwner: true };
test('personal memory belongs to OpenClaw and is absent from the secretary tool surface',async()=>{
 const bridge=createSecretary({coreToken:'synthetic',owner:'+628123'});
 expect(secretaryToolNames.some(name=>name.startsWith('memory_'))).toBe(false);
 for(const name of ['memory_capabilities','memory_search','memory_get','memory_save','memory_update','memory_archive','memory_history']) {
  await expect(bridge.execute(name,{},owner)).rejects.toThrow('UNKNOWN_TOOL');
 }
});
function setup(handler: (request: Request) => Response | Promise<Response>) {
 const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: handler });
 return { server, options: { coreUrl: server.url.origin, coreToken: 'private-token', owner: '+628123' } };
}
(process.env.SECRETARY_HTTP_TESTS==='1'?test:test.skip)('owner reads approved sheets through authenticated Core route', async () => {
 const {server, options} = setup(async request => {
  expect(new URL(request.url).pathname).toBe('/v1/tools/sheets_list');
  expect(request.headers.get('authorization')).toBe('Bearer private-token');
  expect(await request.json()).toEqual({});
  return Response.json({ok:true,data:[{spreadsheet:'wedding'}]});
 });
 try { expect(await createSecretary(options).execute('sheets_list', {}, owner)).toEqual({ok:true,data:[{spreadsheet:'wedding'}]}); }
 finally { server.stop(true); }
});

(process.env.SECRETARY_HTTP_TESTS==='1'?test:test.skip)('revoked owner invocation cannot return a pending Core read',async()=>{
 let current=true;
 const {server,options}=setup(()=>{
  current=false;
  return Response.json({ok:true,data:[{spreadsheet:'private-owner-data'}]});
 });
 try {
  const identity={...owner,assertInvocationCurrent(){if(!current)throw Error('INVOCATION_EXPIRED');}};
  await expect(createSecretary(options).execute('sheets_list',{},identity)).rejects.toThrow('INVOCATION_EXPIRED');
 } finally {server.stop(true);}
});

test('revoked owner invocation cannot return a pending native reminder list',async()=>{
 let current=true;
 const gateway={isAvailable:()=>true,async request():Promise<any>{
  current=false;
  return {jobs:[{id:'owner-job',declarationKey:'secretary:+628123:private'}]};
 }};
 const bridge=createSecretary({coreToken:'synthetic',owner:'+628123',gateway,schedulerQualified:true});
 const identity={...owner,assertInvocationCurrent(){if(!current)throw Error('INVOCATION_EXPIRED');}};
 await expect(bridge.execute('reminders_list',{},identity)).rejects.toThrow('INVOCATION_EXPIRED');
});
test('unknown owner, injected credentials, unqualified writes and memory fail closed', async () => {
 const bridge = createSecretary({coreUrl:'http://127.0.0.1:1',coreToken:'secret',owner:'+628123'});
 await expect(bridge.execute('sheets_list',{},{})).rejects.toThrow('OWNER_REQUIRED');
 await expect(bridge.execute('sheets_list',{coreToken:'attack'},owner)).rejects.toThrow('INVALID_INPUT');
 await expect(bridge.execute('sheets_append',{operation_id:'op1'},owner)).rejects.toThrow('WRITES_DISABLED');
 await expect(bridge.execute('memory_save',{text:'secret'},owner)).rejects.toThrow('UNKNOWN_TOOL');
 expect(() => createSecretary({coreUrl:'http://example.com',coreToken:'secret',owner:'+628123'})).toThrow('INVALID_CORE_DESTINATION');
});
test('owner creates and reconciles a declared native reminder and sees separate run evidence', async () => {
 const jobs: any[]=[];
 const gateway = {isAvailable:()=>true, async request(method:string, params:any):Promise<any> {
  if(method==='cron.list') return {jobs};
  if(method==='cron.add') {const job={...params,id:'native-1'}; jobs.push(job);return {created:true,job};}
  if(method==='cron.runs') return {entries:[{status:'ok',completionStatus:'completed',delivered:false,deliveryStatus:'not-delivered'}]};
  throw Error('unexpected RPC');
 }};
 const bridge=createSecretary({coreUrl:'http://127.0.0.1:1',coreToken:'secret',owner:'+628123',gateway,schedulerQualified:true});
 const input={reminder_id:'deposit-1',text:'Check deposit',schedule:{kind:'cron',expr:'0 9 * * *'}};
 expect(await bridge.execute('reminders_create',input,owner)).toMatchObject({job:{id:'native-1',schedule:{tz:'Asia/Makassar'},payload:{kind:'systemEvent'}}});
 expect(await bridge.execute('reminders_create',input,owner)).toMatchObject({reconciled:true,job:{id:'native-1'}});
 expect(await bridge.execute('reminders_history',{reminder_id:'deposit-1'},owner)).toMatchObject({entries:[{status:'ok',delivered:false}]});
 await expect(bridge.execute('reminders_create',{...input,to:'+other'},owner)).rejects.toThrow('INVALID_INPUT');
 await expect(bridge.execute('reminders_create',{...input,schedule:{kind:'at',at:'next month'}},owner)).rejects.toThrow('EXACT_SCHEDULE_REQUIRED');
});
test('native reminder lifecycle uses native IDs and revision-shaped patches', async () => {
 const job={id:'job-42',sessionTarget:'main',payload:{kind:'systemEvent',text:'Sample reminder'},configRevision:'fixture-revision',declarationKey:'secretary:+628123:deposit',schedule:{kind:'at',at:'2027-01-01T00:00:00.000Z'},enabled:true};
 const gateway={isAvailable:()=>true,async request(method:string,params:any):Promise<any> {
  if(method==='cron.list') return {jobs:[job,{id:'outsider',declarationKey:'secretary:+other:private'}]};
  expect(params.id).toBe('job-42');
  if(method==='cron.get') return job;
  if(method==='cron.update') {Object.assign(job,params.patch);return job;}
  if(method==='cron.remove') return {ok:true,removed:true};
  throw Error('unexpected');
 }};
 const bridge=createSecretary({coreUrl:'http://127.0.0.1:1',coreToken:'secret',owner:'+628123',gateway,schedulerQualified:true});
 expect(await bridge.execute('reminders_list',{},owner)).toMatchObject({jobs:[{id:'job-42'}]});
 expect(await bridge.execute('reminders_get',{reminder_id:'deposit'},owner)).toMatchObject({job:{id:'job-42'}});
 expect(await bridge.execute('reminders_pause',{reminder_id:'deposit'},owner)).toMatchObject({enabled:false});
 expect(await bridge.execute('reminders_resume',{reminder_id:'deposit'},owner)).toMatchObject({enabled:true});
 expect(await bridge.execute('reminders_reschedule',{reminder_id:'deposit',schedule:{kind:'at',at:'2027-02-01T08:00:00+08:00'}},owner)).toMatchObject({schedule:{at:'2027-02-01T00:00:00.000Z'}});
 expect(await bridge.execute('reminders_cancel',{reminder_id:'deposit'},owner)).toMatchObject({id:'job-42',enabled:false,deleteAfterRun:false,description:'Secretary: canceled; retained for history'});
 await expect(bridge.execute('reminders_get',{reminder_id:'private'},owner)).rejects.toThrow('REMINDER_NOT_FOUND');
});
test('unqualified approval transport never issues a permission callback', async () => {
 const bridge=createSecretary({coreUrl:'http://127.0.0.1:1',coreToken:'secret',owner:'+628123'});
 await expect(bridge.prepareApproval('sheets_append',{operation_id:'op1'},owner)).rejects.toThrow('WRITES_DISABLED');
 await expect(bridge.prepareApproval('sheets_append',{operation_id:'op1'},{})).rejects.toThrow('OWNER_REQUIRED');
});
(process.env.SECRETARY_HTTP_TESTS==='1'?test:test.skip)('immutable preview approval reaches separate authenticated endpoint once', async () => {
 const {server,options}=setup(async request=>{
  const path=new URL(request.url).pathname;
  if(path==='/v1/tools/sheets_preview_change') return Response.json({ok:true,data:{operation_id:'op1',actor:'+628123',spreadsheet:'wedding',view:'payments',fields:{amount:42},action:'append',preview_digest:'core-digest',expires_at:'2099-01-01T00:00:00Z',before:{},after:{amount:42}}});
  expect(path).toBe('/v1/operations/op1/approval');
  expect(request.headers.get('authorization')).toBe('Bearer approval-secret');
  expect(await request.json()).toEqual({decision:'approve',preview_digest:'core-digest'});
  return Response.json({ok:true,data:{state:'approved'}});
 });
 try {
  const bridge=createSecretary({...options,approvalToken:'approval-secret',approvalTransportQualified:true});
  const shown=await bridge.execute('sheets_preview_change',{operation_id:'op1',spreadsheet:'wedding',view:'payments',action:'append',fields:{amount:42}},owner) as any;
  shown.data.preview_digest='model-altered';
  const approval=await bridge.prepareApproval('sheets_append',{operation_id:'op1'},owner);
  expect(approval?.allowedDecisions).toEqual(['allow-once','deny']);
  expect(await approval!.onResolution('allow-once')).toBeUndefined();
  await expect(approval!.onResolution('allow-once')).rejects.toThrow('APPROVAL_ALREADY_RESOLVED');
 } finally {server.stop(true);}
});
test('oversized requests and previews without Core evidence never cross the boundary', async () => {
 const bridge=createSecretary({coreUrl:'http://127.0.0.1:1',coreToken:'secret',owner:'+628123',approvalToken:'separate',approvalTransportQualified:true});
 await expect(bridge.execute('sheets_preview_change',{operation_id:'op1',spreadsheet:'wedding',view:'payments',action:'append',fields:{notes:'x'.repeat(66000)}},owner)).rejects.toThrow('REQUEST_TOO_LARGE');
 await expect(bridge.prepareApproval('sheets_append',{operation_id:'op1'},owner)).rejects.toThrow('PREVIEW_REQUIRED');
 await expect(bridge.execute('reminders_list',{},owner)).rejects.toThrow('SCHEDULER_UNAVAILABLE');
 await expect(bridge.execute('memory_capabilities',{},{})).rejects.toThrow('OWNER_REQUIRED');
});
(process.env.SECRETARY_HTTP_TESTS==='1'?test:test.skip)('Core lowercase error codes survive sanitization while raw provider messages stay hidden',async()=>{
 const {server,options}=setup(()=>Response.json({ok:false,error:{code:'spreadsheet_not_registered',message:'private upstream diagnostic',retryable:false}},{status:404}));
 try {
  expect(await createSecretary(options).execute('sheets_describe',{spreadsheet:'denied'},owner)).toEqual({ok:false,error:{code:'spreadsheet_not_registered',message:'Core rejected the request',retryable:false}});
 } finally {server.stop(true);}
});

test('owner-only configuration defaults to the Core listener without requiring a URL', async () => {
 const bridge=createSecretary({coreToken:'synthetic-secret',owner:'+628123'});
 await expect(bridge.execute('sheets_append',{operation_id:'disabled'},owner)).rejects.toThrow('WRITES_DISABLED');
 const manifest=await Bun.file(new URL('../openclaw.plugin.json',import.meta.url)).json();
 expect(manifest.configSchema.properties.coreUrl.default).toBe('http://127.0.0.1:8787');
});

test('canceled reminders retain history and stay canceled across bridge restarts and create retries', async () => {
 const jobs:any[]=[];
 const entries=[{jobId:'native-retained',status:'ok',delivered:false,deliveryStatus:'not-delivered'}];
 const gateway={isAvailable:()=>true,async request(method:string,params:any):Promise<any> {
  if(method==='cron.list') return {jobs:structuredClone(jobs)};
  if(method==='cron.add') {const job={...structuredClone(params),id:'native-retained',configRevision:'fixture-revision',enabled:true};jobs.push(job);return {created:true,job};}
  const job=jobs.find(j=>j.id===params.id);
  if(method==='cron.update') {Object.assign(job,params.patch);return structuredClone(job);}
  if(method==='cron.remove') {jobs.splice(jobs.indexOf(job),1);return {ok:true,removed:true};}
  if(method==='cron.runs') return {entries};
  throw Error('unexpected RPC');
 }};
 const options={coreToken:'synthetic-secret',owner:'+628123',gateway,schedulerQualified:true};
 const input={reminder_id:'retained',text:'Check payment',schedule:{kind:'at',at:'2027-01-01T00:00:00Z'}};
 const bridge=createSecretary(options);
 await bridge.execute('reminders_create',input,owner);
 await bridge.execute('reminders_cancel',{reminder_id:'retained'},owner);
 const restarted=createSecretary(options);
 expect(await restarted.execute('reminders_history',{reminder_id:'retained'},owner)).toEqual({entries});
 expect(await restarted.execute('reminders_create',input,owner)).toMatchObject({reconciled:true,job:{id:'native-retained',enabled:false}});
});

test('one-time reminders retain their native identity and completed history without reactivation', async () => {
 let job:any;
 const entries=[{jobId:'one-time-job',status:'ok',delivered:true,deliveryStatus:'delivered'}];
 const gateway={isAvailable:()=>true,async request(method:string,params:any):Promise<any> {
  if(method==='cron.list') return {jobs:job?[structuredClone(job)]:[]};
  if(method==='cron.add') {job={...structuredClone(params),id:'one-time-job',enabled:true};return {created:true,job:structuredClone(job)};}
  if(method==='cron.runs') return {entries};
  throw Error('unexpected RPC');
 }};
 const options={coreToken:'synthetic-secret',owner:'+628123',gateway,schedulerQualified:true};
 const input={reminder_id:'one-time',text:'Check venue payment',schedule:{kind:'at',at:'2027-01-01T00:00:00Z'}};
 const bridge=createSecretary(options);
 expect(await bridge.execute('reminders_create',input,owner)).toMatchObject({job:{deleteAfterRun:false}});
 // Native completion snapshot: retained one-time jobs are disabled by the scheduler.
 job.enabled=false;job.state={lastRunStatus:'ok',lastDelivered:true};
 const restarted=createSecretary(options);
 expect(await restarted.execute('reminders_history',{reminder_id:'one-time'},owner)).toEqual({entries});
 expect(await restarted.execute('reminders_create',input,owner)).toMatchObject({reconciled:true,job:{id:'one-time-job',enabled:false}});
});

test('reminders beyond the first scheduler page reconcile without being recreated', async () => {
 const target={id:'later-job',declarationKey:'secretary:+628123:later',configRevision:'rev-later',enabled:false,sessionTarget:'main',schedule:{kind:'cron',expr:'0 9 * * *',tz:'Asia/Makassar'},payload:{kind:'systemEvent',text:'Retained reminder'}};
 const gateway={isAvailable:()=>true,async request(method:string,params:any):Promise<any> {
  if(method==='cron.list') return params.offset===200?{jobs:[target],hasMore:false,nextOffset:null}:{jobs:Array.from({length:200},(_,i)=>({id:`other-${i}`,declarationKey:`other:${i}`})),hasMore:true,nextOffset:200};
  if(method==='cron.add') throw Error('must not duplicate an existing reminder');
  throw Error('unexpected RPC');
 }};
 const bridge=createSecretary({coreToken:'synthetic',owner:'+628123',gateway,schedulerQualified:true});
 expect(await bridge.execute('reminders_create',{reminder_id:'later',text:'Retained reminder',schedule:{kind:'cron',expr:'0 9 * * *'}},owner)).toMatchObject({reconciled:true,job:{id:'later-job',enabled:false}});
 expect(await bridge.execute('reminders_list',{},owner)).toEqual({jobs:[target]});
});

test('reminder updates use the observed revision and preserve a concurrent native edit', async () => {
 let race=false;
 const job:any={id:'revision-job',sessionTarget:'main',payload:{kind:'systemEvent',text:'Sample reminder'},declarationKey:'secretary:+628123:revision',configRevision:'v1',enabled:true,schedule:{kind:'cron',expr:'0 9 * * *',tz:'Asia/Makassar'}};
 const gateway={isAvailable:()=>true,async request(method:string,params:any):Promise<any> {
  if(method==='cron.list') {const snapshot=structuredClone(job);if(race){race=false;job.configRevision='owner-edit';job.schedule.expr='0 10 * * *';}return {jobs:[snapshot]};}
  if(method==='cron.get') return structuredClone(job);
  if(method==='cron.update') {
   if(params.expectedConfigRevision!==job.configRevision) throw Error('revision conflict');
   Object.assign(job,params.patch,{configRevision:'v2'});return structuredClone(job);
  }
  throw Error('unexpected RPC');
 }};
 const bridge=createSecretary({coreToken:'synthetic',owner:'+628123',gateway,schedulerQualified:true});
 expect(await bridge.execute('reminders_pause',{reminder_id:'revision'},owner)).toMatchObject({enabled:false});
 race=true;
 await expect(bridge.execute('reminders_reschedule',{reminder_id:'revision',schedule:{kind:'cron',expr:'0 11 * * *'}},owner)).rejects.toThrow('revision conflict');
 expect(await bridge.execute('reminders_get',{reminder_id:'revision'},owner)).toMatchObject({job:{schedule:{expr:'0 10 * * *'},configRevision:'owner-edit'}});
});

test('unversioned native jobs remain readable but cannot be changed', async () => {
 let modified=false;
 const job={id:'legacy-job',declarationKey:'secretary:+628123:legacy',enabled:true};
 const gateway={isAvailable:()=>true,async request(method:string):Promise<any> {
  if(method==='cron.list') return {jobs:[job]};
  if(method==='cron.get') return job;
  modified=true;return {};
 }};
 const bridge=createSecretary({coreToken:'synthetic',owner:'+628123',gateway,schedulerQualified:true});
 expect(await bridge.execute('reminders_get',{reminder_id:'legacy'},owner)).toEqual({job});
 await expect(bridge.execute('reminders_pause',{reminder_id:'legacy'},owner)).rejects.toThrow('REMINDER_REVISION_REQUIRED');
 expect(modified).toBe(false);
});

test('expired invocations cannot create or change reminders after scheduler discovery', async () => {
 for(const action of ['create','pause']) {
  let active=true,modified=false;
  const gateway={isAvailable:()=>true,async request(method:string):Promise<any> {
   if(method==='cron.list') {active=false;return {jobs:action==='create'?[]:[{id:'stale-job',declarationKey:'secretary:+628123:stale',configRevision:'v1'}]};}
   modified=true;return {id:'stale-job'};
  }};
  const bridge=createSecretary({coreToken:'synthetic',owner:'+628123',gateway,schedulerQualified:true});
  const identity={...owner,assertInvocationCurrent(){if(!active)throw Error('INVOCATION_EXPIRED');}};
  const input=action==='create'?{reminder_id:'stale',text:'Sample reminder',schedule:{kind:'cron',expr:'0 9 * * *'}}:{reminder_id:'stale'};
  await expect(bridge.execute(`reminders_${action}`,input,identity)).rejects.toThrow('INVOCATION_EXPIRED');
  expect(modified).toBe(false);
 }
});

test('incomplete or unstable scheduler discovery cannot authorize a new reminder', async () => {
 const scenarios=[
  {error:'INVALID_GATEWAY_RESPONSE',page:(offset:number)=>({jobs:[{id:'bad-offset'}],hasMore:true,nextOffset:offset})},
  {error:'SCHEDULER_SNAPSHOT_CHANGED',page:(offset:number)=>({jobs:[{id:'repeated-job'}],hasMore:offset===0,nextOffset:offset===0?200:null})},
  {error:'SCHEDULER_LIST_TOO_LARGE',page:(offset:number)=>({jobs:[{id:`job-${offset}`}],hasMore:true,nextOffset:offset+1})},
 ];
 for(const scenario of scenarios) {
  let modified=false;
  const gateway={isAvailable:()=>true,async request(method:string,params:any):Promise<any> {
   if(method==='cron.list') return scenario.page(params.offset??0);
   modified=true;return {};
  }};
  const bridge=createSecretary({coreToken:'synthetic',owner:'+628123',gateway,schedulerQualified:true});
  await expect(bridge.execute('reminders_create',{reminder_id:'must-wait',text:'Sample reminder',schedule:{kind:'cron',expr:'0 9 * * *'}},owner)).rejects.toThrow(scenario.error);
  expect(modified).toBe(false);
 }
});

test('changed native payloads or delivery routes cannot be reactivated through secretary tools', async () => {
 for(const changed of [
  {sessionTarget:'isolated',payload:{kind:'agentTurn',message:'Changed workflow'}},
  {delivery:{mode:'announce',channel:'whatsapp',to:'+other'}},
 ]) {
  let modified=false;
  const job={id:'changed-job',declarationKey:'secretary:+628123:changed',configRevision:'v1',sessionTarget:'main',payload:{kind:'systemEvent',text:'Original reminder'},enabled:false,...changed};
  const gateway={isAvailable:()=>true,async request(method:string):Promise<any> {
   if(method==='cron.list') return {jobs:[job]};
   modified=true;return {...job,enabled:true};
  }};
  const bridge=createSecretary({coreToken:'synthetic',owner:'+628123',gateway,schedulerQualified:true});
  await expect(bridge.execute('reminders_resume',{reminder_id:'changed'},owner)).rejects.toThrow('REMINDER_CONFLICT');
  expect(modified).toBe(false);
 }
});

test('native asynchronous Gateway availability denies access before issuing scheduler RPC',async()=>{
 let called=false;
 const gateway={async isAvailable(){return false;},async request():Promise<never>{called=true;throw Error('must not call unavailable Gateway');}};
 const bridge=createSecretary({coreToken:'synthetic',owner:'+628123',gateway,schedulerQualified:true});
 await expect(bridge.execute('reminders_list',{},owner)).rejects.toThrow('SCHEDULER_UNAVAILABLE');
 expect(called).toBe(false);
});

test('available asynchronous Gateway preserves invocation checks before scheduler access',async()=>{
 let current=true,requests=0;
 const gateway={async isAvailable(){return true;},async request<T>():Promise<T>{requests++;return {jobs:[]} as T;}};
 const bridge=createSecretary({coreToken:'synthetic',owner:'+628123',gateway,schedulerQualified:true});
 const identity={...owner,assertInvocationCurrent(){if(!current)throw Error('INVOCATION_EXPIRED');}};
 expect(await bridge.execute('reminders_list',{},identity)).toEqual({jobs:[]});
 gateway.isAvailable=async()=>{current=false;return true;};
 await expect(bridge.execute('reminders_list',{},identity)).rejects.toThrow('INVOCATION_EXPIRED');
 expect(requests).toBe(1);
});
