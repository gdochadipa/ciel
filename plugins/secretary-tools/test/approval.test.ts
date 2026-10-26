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
  const revoked=(await bridge.prepareApproval('sheets_update',{operation_id:'approval-1'},identity))!;
  current=false;
  await expect(revoked.onResolution('allow-once')).rejects.toThrow('INVOCATION_EXPIRED');
  expect(transport).toHaveBeenCalledTimes(1);
 } finally {clock.mockRestore();transport.mockRestore();}
});
