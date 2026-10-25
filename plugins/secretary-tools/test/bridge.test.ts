import { test, expect } from 'bun:test';
import { createSecretary } from '../src/bridge';
const owner = { requesterSenderId: '+628123', senderIsOwner: true };
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
test('unknown owner, injected credentials, unqualified writes and memory fail closed', async () => {
 const bridge = createSecretary({coreUrl:'http://127.0.0.1:1',coreToken:'secret',owner:'+628123'});
 await expect(bridge.execute('sheets_list',{},{})).rejects.toThrow('OWNER_REQUIRED');
 await expect(bridge.execute('sheets_list',{coreToken:'attack'},owner)).rejects.toThrow('INVALID_INPUT');
 await expect(bridge.execute('sheets_append',{operation_id:'op1'},owner)).rejects.toThrow('WRITES_DISABLED');
 await expect(bridge.execute('memory_save',{text:'secret'},owner)).rejects.toThrow('MEMORY_UNAVAILABLE');
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
 const job={id:'job-42',declarationKey:'secretary:+628123:deposit',schedule:{kind:'at',at:'2027-01-01T00:00:00.000Z'},enabled:true};
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
 expect(await bridge.execute('reminders_cancel',{reminder_id:'deposit'},owner)).toEqual({ok:true,removed:true});
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
  if(path==='/v1/tools/sheets_preview_change') return Response.json({ok:true,data:{operation_id:'op1',actor:'owner',preview_digest:'core-digest',expires_at:'2099-01-01T00:00:00Z',before:{},after:{amount:42}}});
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
  expect(await approval!.onResolution('allow-once')).toMatchObject({ok:true});
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
