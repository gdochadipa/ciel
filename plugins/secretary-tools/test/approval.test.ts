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

test('Docs uses the same immutable one-shot approval and cannot cross into a Sheets commit',async()=>{
 const docChange={operation_id:'docs-approval',document:'notes',tab_id:'t.0',action:'replace_text',fields:{find:'sample',replacement:'team'}};
 const docPreview={...docChange,resource_kind:'google_docs',actor:owner.requesterSenderId,before:{text:'Hello sample\n'},after:{text:'Hello team\n'},record_id:'registered-doc',mapping_version:'v1',source_revision:'revision-1',preview_digest:'docs-digest',expires_at:'2099-01-01T00:00:00Z',state:'previewed'};
 const requests:Request[]=[];
 const transport=spyOn(globalThis,'fetch').mockImplementation(async(input,init)=>{
  requests.push(new Request(input,init));
  return Response.json(requests.length===1?{ok:true,data:docPreview}:{ok:true,data:{state:'approved'}});
 });
 try{
  const bridge=createSecretary(options);
  const shown=await bridge.execute('docs_preview_change',docChange,owner) as any;shown.data.after.text='Model changed';
  await expect(bridge.prepareApproval('sheets_update',{operation_id:docChange.operation_id},owner)).rejects.toThrow('PREVIEW_ACTION_MISMATCH');
  const approval=(await bridge.prepareApproval('docs_apply',{operation_id:docChange.operation_id},owner))!;
  expect(approval.description).toContain(JSON.stringify(docPreview,null,2));
  await approval.onResolution('allow-once');await expect(approval.onResolution('allow-once')).rejects.toThrow('APPROVAL_ALREADY_RESOLVED');
  expect(await requests[1].json()).toEqual({decision:'approve',preview_digest:'docs-digest'});
  expect(requests[1].headers.get('authorization')).toBe('Bearer synthetic-approval');
 }finally{transport.mockRestore();}
});

test('Docs preview binding rejects another resource or actor before caching',async()=>{
 const change={operation_id:'docs-bad',document:'notes',tab_id:'t.0',action:'append_text',fields:{text:'Test'}};
 for(const altered of [{resource_kind:'sheets'},{document:'another'},{tab_id:'t.1'},{actor:'another'},{action:'replace_text'}]){
  const transport=spyOn(globalThis,'fetch').mockResolvedValue(Response.json({ok:true,data:{...change,resource_kind:'google_docs',actor:owner.requesterSenderId,expires_at:'2099-01-01T00:00:00Z',preview_digest:'digest',...altered}}));
  try{
   const bridge=createSecretary(options);await expect(bridge.execute('docs_preview_change',change,owner)).rejects.toThrow('INVALID_CORE_RESPONSE');
   await expect(bridge.prepareApproval('docs_apply',{operation_id:change.operation_id},owner)).rejects.toThrow('PREVIEW_REQUIRED');
  }finally{transport.mockRestore();}
 }
});

test('unqualified Docs apply and injected provider controls fail before traffic',async()=>{
 const transport=spyOn(globalThis,'fetch').mockRejectedValue(Error('Must not be called'));
 try{
  const bridge=createSecretary({...options,approvalTransportQualified:false});
  await expect(bridge.execute('docs_apply',{operation_id:'docs-denied'},owner)).rejects.toThrow('WRITES_DISABLED');
  await expect(bridge.execute('docs_read',{document:'notes',tab_id:'t.0',url:'https://example.com'},owner)).rejects.toThrow('INVALID_INPUT');
  await expect(bridge.execute('docs_preview_change',{operation_id:'docs-denied',document:'notes',tab_id:'t.0',action:'arbitrary_batch',fields:{}},owner)).rejects.toThrow('INVALID_INPUT');
  expect(transport).not.toHaveBeenCalled();
 }finally{transport.mockRestore();}
});

test('tab creation binds literal headers and target to a separate one-shot approval',async()=>{
 const tab={operation_id:'tab-approval',spreadsheet:'operations',title:'Plan',rows:20,columns:3,headers:['Task','=literal']};
 const p={operation_id:tab.operation_id,spreadsheet:tab.spreadsheet,view:tab.title,fields:{title:tab.title,rows:20,columns:3,headers:tab.headers},resource_kind:'sheets_tab',action:'create_tab',actor:owner.requesterSenderId,after:{sheet_id:88,registration:'unregistered',readable:false,writable:false},expires_at:'2099-01-01T00:00:00Z',preview_digest:'tab-digest'};
 const requests:Request[]=[];
 const transport=spyOn(globalThis,'fetch').mockImplementation(async(input,init)=>{requests.push(new Request(input,init));return Response.json({ok:true,data:requests.length===1?p:{state:'approved'}});});
 try{
  const bridge=createSecretary(options);
  await bridge.execute('sheets_preview_create_tab',tab,owner);
  await expect(bridge.prepareApproval('sheets_append',{operation_id:tab.operation_id},owner)).rejects.toThrow('PREVIEW_ACTION_MISMATCH');
  const approval=(await bridge.prepareApproval('sheets_create_tab',{operation_id:tab.operation_id},owner))!;
  expect(approval.description).toContain(JSON.stringify(p,null,2));await approval.onResolution('allow-once');
  expect(await requests[1].json()).toEqual({decision:'approve',preview_digest:'tab-digest'});
  await expect(approval.onResolution('allow-once')).rejects.toThrow('APPROVAL_ALREADY_RESOLVED');
  await expect(createSecretary({...options,approvalTransportQualified:false}).execute('sheets_create_tab',{operation_id:tab.operation_id},owner)).rejects.toThrow('WRITES_DISABLED');
 }finally{transport.mockRestore();}
});

test('tab preview rejects target/owner substitution and bounded input violations before approval',async()=>{
 const tab={operation_id:'tab-bound',spreadsheet:'operations',title:'Plan',rows:20,columns:3};
 const p={operation_id:tab.operation_id,spreadsheet:tab.spreadsheet,fields:{title:'Plan',rows:20,columns:3,headers:[]},resource_kind:'sheets_tab',action:'create_tab',actor:owner.requesterSenderId,expires_at:'2099-01-01T00:00:00Z',preview_digest:'digest'};
 for(const altered of [{actor:'another'},{spreadsheet:'another'},{resource_kind:'google_docs'},{fields:{...p.fields,rows:21}}]){
  const transport=spyOn(globalThis,'fetch').mockResolvedValue(Response.json({ok:true,data:{...p,...altered}}));
  try{const bridge=createSecretary(options);await expect(bridge.execute('sheets_preview_create_tab',tab,owner)).rejects.toThrow('INVALID_CORE_RESPONSE');await expect(bridge.prepareApproval('sheets_create_tab',{operation_id:tab.operation_id},owner)).rejects.toThrow('PREVIEW_REQUIRED');}finally{transport.mockRestore();}
 }
 const transport=spyOn(globalThis,'fetch').mockRejectedValue(Error('Unexpected traffic'));
 try{
  for(const altered of [{rows:1001},{rows:1000,columns:26},{title:'Bad/name'},{headers:['a','b','c','d']},{headers:['😀'.repeat(65)]},{sheet_id:3},{approved:true}])await expect(createSecretary(options).execute('sheets_preview_create_tab',{...tab,...altered},owner)).rejects.toThrow('INVALID_INPUT');
  expect(transport).not.toHaveBeenCalled();
 }finally{transport.mockRestore();}
});
