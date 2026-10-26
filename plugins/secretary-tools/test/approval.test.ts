import {test,expect,spyOn} from 'bun:test';
import {createSecretary} from '../src/bridge';

// Fetch-boundary fixtures only: these do not start Core or qualify host approvals.
const owner={requesterSenderId:'+628123',senderIsOwner:true};
const change={operation_id:'approval-1',spreadsheet:'wedding',view:'tasks',action:'update',record_id:'task-1',fields:{status:'done'}};
const preview={...change,actor:'+628123',before:{title:'Check deposit',status:'todo'},after:{title:'Check deposit',status:'done'},mapping_version:'v1',source_revision:'source-1',preview_digest:'digest-1',expires_at:'2099-01-01T00:00:00Z',state:'previewed'};
const options={coreToken:'synthetic-core',approvalToken:'synthetic-approval',owner:'+628123',approvalTransportQualified:true};

test('approval presents the complete immutable Core preview and sends its original digest once',async()=>{
 const requests:Request[]=[];
 const transport=spyOn(globalThis,'fetch').mockImplementation(async(input,init)=>{
  const request=new Request(input,init);requests.push(request);
  return Response.json(requests.length===1?{ok:true,data:preview}:{ok:true,data:{state:'approved'}});
 });
 try {
  const bridge=createSecretary(options);
  const shown=await bridge.execute('sheets_preview_change',change,owner) as {data:typeof preview};
  shown.data.after.status='model-altered';shown.data.preview_digest='model-altered';
  const approval=(await bridge.prepareApproval('sheets_update',{operation_id:'approval-1'},owner))!;
  expect(approval.description).toContain(JSON.stringify(preview,null,2));
  expect(approval.description).not.toContain('model-altered');
  expect(approval.severity).toBe('warning');
  expect(approval.allowedDecisions).toEqual(['allow-once','deny']);
  await approval.onResolution('allow-once');
  expect(requests[1].url).toBe('http://127.0.0.1:8787/v1/operations/approval-1/approval');
  expect(requests[1].headers.get('authorization')).toBe('Bearer synthetic-approval');
  expect(await requests[1].json()).toEqual({decision:'approve',preview_digest:'digest-1'});
  await expect(approval.onResolution('allow-once')).rejects.toThrow('APPROVAL_ALREADY_RESOLVED');
  expect(requests).toHaveLength(2);
 } finally {transport.mockRestore();}
});

test('approval cannot apply an update preview to an append tool call',async()=>{
 const transport=spyOn(globalThis,'fetch').mockResolvedValue(Response.json({ok:true,data:preview}));
 try {
  const bridge=createSecretary(options);
  await bridge.execute('sheets_preview_change',change,owner);
  await expect(bridge.prepareApproval('sheets_append',{operation_id:'approval-1'},owner)).rejects.toThrow('PREVIEW_ACTION_MISMATCH');
  expect(transport).toHaveBeenCalledTimes(1);
 } finally {transport.mockRestore();}
});

test('Core refusal stops approval resolution and cannot be retried through the callback',async()=>{
 const transport=spyOn(globalThis,'fetch')
  .mockResolvedValueOnce(Response.json({ok:true,data:preview}))
  .mockResolvedValueOnce(Response.json({ok:false,error:{code:'operation_conflict'}},{status:409}));
 try {
  const bridge=createSecretary(options);
  await bridge.execute('sheets_preview_change',change,owner);
  const approval=(await bridge.prepareApproval('sheets_update',{operation_id:'approval-1'},owner))!;
  await expect(approval.onResolution('allow-once')).rejects.toThrow('APPROVAL_REJECTED');
  await expect(approval.onResolution('allow-once')).rejects.toThrow('APPROVAL_ALREADY_RESOLVED');
  expect(transport).toHaveBeenCalledTimes(2);
 } finally {transport.mockRestore();}
});

test('denial persists rejection with the captured digest and consumes the preview',async()=>{
 const decisions:unknown[]=[];
 const transport=spyOn(globalThis,'fetch').mockImplementation(async(_input,init)=>{
  if(decisions.length===0) {decisions.push('preview');return Response.json({ok:true,data:preview});}
  decisions.push(JSON.parse(String(init?.body)));
  return Response.json({ok:true,data:{state:'rejected'}});
 });
 try {
  const bridge=createSecretary(options);
  await bridge.execute('sheets_preview_change',change,owner);
  const approval=(await bridge.prepareApproval('sheets_update',{operation_id:'approval-1'},owner))!;
  await approval.onResolution('deny');
  expect(decisions).toEqual(['preview',{decision:'reject',preview_digest:'digest-1'}]);
  await expect(approval.onResolution('allow-once')).rejects.toThrow('APPROVAL_ALREADY_RESOLVED');
  await expect(bridge.prepareApproval('sheets_update',{operation_id:'approval-1'},owner)).rejects.toThrow('PREVIEW_REQUIRED');
 } finally {transport.mockRestore();}
});

test('timeout, cancellation and allow-always never reach the operator endpoint',async()=>{
 const transport=spyOn(globalThis,'fetch').mockImplementation(async()=>Response.json({ok:true,data:preview}));
 try {
  for(const decision of ['timeout','cancelled','allow-always'] as const) {
   const bridge=createSecretary(options);
   await bridge.execute('sheets_preview_change',change,owner);
   const approval=(await bridge.prepareApproval('sheets_update',{operation_id:'approval-1'},owner))!;
   await expect(approval.onResolution(decision)).rejects.toThrow('APPROVAL_DENIED');
   await expect(approval.onResolution('allow-once')).rejects.toThrow('APPROVAL_ALREADY_RESOLVED');
   await expect(bridge.prepareApproval('sheets_update',{operation_id:'approval-1'},owner)).rejects.toThrow('PREVIEW_REQUIRED');
  }
  expect(transport).toHaveBeenCalledTimes(3);
 } finally {transport.mockRestore();}
});

test('expiry and revoked invocation authority prevent operator traffic after a dialog is created',async()=>{
 const transport=spyOn(globalThis,'fetch').mockImplementation(async()=>Response.json({ok:true,data:preview}));
 const clock=spyOn(Date,'now').mockReturnValue(Date.parse('2098-12-31T23:59:00Z'));
 try {
  let current=true;
  const identity={...owner,assertInvocationCurrent(){if(!current)throw Error('INVOCATION_EXPIRED');}};
  const bridge=createSecretary(options);
  await bridge.execute('sheets_preview_change',change,identity);
  const expired=(await bridge.prepareApproval('sheets_update',{operation_id:'approval-1'},identity))!;
  clock.mockReturnValue(Date.parse('2099-01-01T00:00:00Z'));
  await expect(expired.onResolution('allow-once')).rejects.toThrow('PREVIEW_EXPIRED');
  await expect(expired.onResolution('allow-once')).rejects.toThrow('APPROVAL_ALREADY_RESOLVED');
  await expect(bridge.prepareApproval('sheets_update',{operation_id:'approval-1'},identity)).rejects.toThrow('PREVIEW_REQUIRED');
  clock.mockReturnValue(Date.parse('2098-12-31T23:59:00Z'));
  await bridge.execute('sheets_preview_change',change,identity);
  const revoked=(await bridge.prepareApproval('sheets_update',{operation_id:'approval-1'},identity))!;
  current=false;
  await expect(revoked.onResolution('allow-once')).rejects.toThrow('INVOCATION_EXPIRED');
  expect(transport).toHaveBeenCalledTimes(2);
 } finally {clock.mockRestore();transport.mockRestore();}
});

test('a long-lived bridge retains at most 64 previews and refreshes an explicitly fetched preview',async()=>{
 const transport=spyOn(globalThis,'fetch').mockImplementation(async(_input,init)=>{
  const input=JSON.parse(String(init?.body));
  return Response.json({ok:true,data:{...preview,operation_id:input.operation_id}});
 });
 try {
  const bridge=createSecretary(options);
  for(let i=0;i<64;i++)await bridge.execute('sheets_preview_change',{...change,operation_id:`bounded-${i}`},owner);
  await bridge.execute('sheets_preview_change',{...change,operation_id:'bounded-0'},owner);
  await bridge.execute('sheets_preview_change',{...change,operation_id:'bounded-64'},owner);
  await expect(bridge.prepareApproval('sheets_update',{operation_id:'bounded-1'},owner)).rejects.toThrow('PREVIEW_REQUIRED');
  expect((await bridge.prepareApproval('sheets_update',{operation_id:'bounded-0'},owner))!.description).toContain('bounded-0');
  expect((await bridge.prepareApproval('sheets_update',{operation_id:'bounded-64'},owner))!.description).toContain('bounded-64');
  expect(transport).toHaveBeenCalledTimes(66);
 } finally {transport.mockRestore();}
});

test('expired previews are reclaimed before an unexpired preview is displaced',async()=>{
 const start=Date.parse('2098-01-01T00:00:00Z');
 const clock=spyOn(Date,'now').mockReturnValue(start);
 const transport=spyOn(globalThis,'fetch').mockImplementation(async(_input,init)=>{
  const input=JSON.parse(String(init?.body));
  return Response.json({ok:true,data:{...preview,operation_id:input.operation_id,expires_at:new Date(start+(input.operation_id==='valid-first'||input.operation_id==='fresh'?60000:1000)).toISOString()}});
 });
 try {
  const bridge=createSecretary(options);
  await bridge.execute('sheets_preview_change',{...change,operation_id:'valid-first'},owner);
  for(let i=0;i<63;i++)await bridge.execute('sheets_preview_change',{...change,operation_id:`short-${i}`},owner);
  clock.mockReturnValue(start+2000);
  await bridge.execute('sheets_preview_change',{...change,operation_id:'fresh'},owner);
  expect((await bridge.prepareApproval('sheets_update',{operation_id:'valid-first'},owner))!.description).toContain('valid-first');
  await expect(bridge.prepareApproval('sheets_update',{operation_id:'short-0'},owner)).rejects.toThrow('PREVIEW_REQUIRED');
 } finally {clock.mockRestore();transport.mockRestore();}
});

test('large previews share a four MiB serialized cache budget without corrupting an open approval',async()=>{
 const transport=spyOn(globalThis,'fetch').mockImplementation(async(_input,init)=>{
  const input=JSON.parse(String(init?.body));
  if(!input.operation_id)return Response.json({ok:true,data:{state:'approved'}});
  return Response.json({ok:true,data:{...preview,operation_id:input.operation_id,after:{note:'x'.repeat(900000)}}});
 });
 try {
  const bridge=createSecretary(options);
  await bridge.execute('sheets_preview_change',{...change,operation_id:'large-0'},owner);
  const opened=(await bridge.prepareApproval('sheets_update',{operation_id:'large-0'},owner))!;
  for(let i=1;i<5;i++)await bridge.execute('sheets_preview_change',{...change,operation_id:`large-${i}`},owner);
  await expect(bridge.prepareApproval('sheets_update',{operation_id:'large-0'},owner)).rejects.toThrow('PREVIEW_REQUIRED');
  expect((await bridge.prepareApproval('sheets_update',{operation_id:'large-4'},owner))!.description).toContain('large-4');
  await opened.onResolution('allow-once');
  await expect(opened.onResolution('allow-once')).rejects.toThrow('APPROVAL_ALREADY_RESOLVED');
  expect(transport).toHaveBeenCalledTimes(6);
 } finally {transport.mockRestore();}
});
