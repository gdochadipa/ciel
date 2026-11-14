import {test,expect,spyOn} from 'bun:test';
import {createNativeTaskClient} from '../src/task-native';
const id='11111111-1111-4111-8111-111111111111';
const nonce='22222222-2222-4222-8222-222222222222';
const digest='a'.repeat(64);
const scope={objective:'Fix sample',mapping_version:'v1',max_operations:10,targets:[{kind:'document',document:'notes',document_id:'doc',tab_id:'t.0'}]};
let nextPort=18000;
function fixture(){
 const coreUrl='http://127.0.0.1:'+nextPort++;
 let active=true,owner=true,stage='proposed',revision=0;
 const paths:string[]=[],bodies:Record<string,unknown>[]=[];
 let response:Response|undefined,streamMember:unknown;
 const context={agentId:'agent',sessionKey:'agent:owner',sessionId:'incarnation',requesterSenderId:'owner',get senderIsOwner(){return owner;},assertInvocationCurrent(){if(!active)throw Error('INVOCATION_EXPIRED');}};
 const fetch=spyOn(globalThis,'fetch').mockImplementation(async(input,init)=>{
  const request=new Request(input,init),path=new URL(request.url).pathname,body:Record<string,unknown>=await request.json();
  expect(request.headers.get('authorization')).toBe('Bearer synthetic-native-token');paths.push(path);bodies.push(body);
  if(response!==undefined)return response;
  if(path.endsWith('/native')){
   if(body.action==='decide'){stage=body.decision==='approve'?'active':'closed';revision++;}
   return Response.json({ok:true,owner:'owner',data:{schema:'ciel.write-task.v1',task_id:id,scope,scope_digest:digest,grant_revision:revision,phase:stage}});
  }
  if(path.endsWith('/execute')){
   streamMember=body;
   return new Response(JSON.stringify({type:'ready',owner:'owner',challenge:{member:body,preview_digest:digest,nonce}})+'\n'+JSON.stringify({type:'result',owner:'owner',ok:true,data:{state:'applied',operation_id:'op',preview_digest:digest}})+'\n',{headers:{'content-type':'application/vnd.ciel.task-dispatch+ndjson;version=1'}});
  }
  if(path.endsWith('/confirm'))return Response.json({ok:true,owner:'owner',data:{confirmed:true}});
  throw Error('UNEXPECTED_ROUTE');
 });
 const client=createNativeTaskClient({qualified:true,coreUrl,owner:'owner',nativeToken:'synthetic-native-token',summaryMemoryEnabled:false},context,new AbortController().signal);
 return {client,context,paths,bodies,fetch,recreate:()=>createNativeTaskClient({qualified:true,coreUrl,owner:'owner',nativeToken:'synthetic-native-token',summaryMemoryEnabled:false},context,new AbortController().signal),member:()=>streamMember,revoke(){active=false;},disown(){owner=false;},response(value:Response|undefined){response=value;}};
}

test('native task proposal captures scope and one owner decision without per-member prompts',async()=>{
 const f=fixture();try{
  const input=structuredClone(scope),proposal=await f.client.propose(input);expect(proposal.taskId).toBe(id);input.objective='Changed after capture';
  expect(proposal.approval.description).toContain('Fix sample');expect(proposal.approval.description).not.toContain('Changed after capture');
  await expect(proposal.grant()).rejects.toThrow('APPROVAL_PENDING');
  await proposal.approval.onResolution('allow-once');const grant=await proposal.grant();expect(grant).toEqual({taskId:id,scopeDigest:digest,revision:1});
  await expect(proposal.approval.onResolution('allow-once')).rejects.toThrow('APPROVAL_ALREADY_RESOLVED');
  await f.client.capture(grant.taskId,{kind:'document',operation_id:'op',document:'notes',tab_id:'t.0',action:'replace_text',fields:{find:'old',replacement:'new'}});
  expect(await f.client.execute(id,grant.revision,'op')).toEqual({type:'result',ok:true,data:{state:'applied',operation_id:'op',preview_digest:digest}});
  expect(f.paths).toEqual(['/v1/write-tasks/native','/v1/write-tasks/native','/v1/write-tasks/native','/v1/write-tasks/execute','/v1/write-tasks/confirm']);
  expect(f.bodies[4]).toMatchObject({member:{context:f.bodies[0]?.context}});
 }finally{f.fetch.mockRestore();}
});

test('retained native task approval cannot acquire authority after reset or abort',async()=>{
 const f=fixture();try{
  const proposal=await f.client.propose(scope);f.revoke();
  await expect(proposal.approval.onResolution('allow-once')).rejects.toThrow('INVOCATION_EXPIRED');
  await expect(proposal.grant()).rejects.toThrow('INVOCATION_EXPIRED');expect(f.paths).toHaveLength(1);
 }finally{f.fetch.mockRestore();}
 const abort=new AbortController();abort.abort();expect(()=>createNativeTaskClient({qualified:true,owner:'owner',nativeToken:'synthetic-native-token',summaryMemoryEnabled:false},{agentId:'a',sessionKey:'s',sessionId:'i',senderIsOwner:true,requesterSenderId:'owner',assertInvocationCurrent(){}},abort.signal)).toThrow('INVOCATION_EXPIRED');
});

test('native denial persists rejection without minting a grant or retrying',async()=>{
 const f=fixture();try{const p=await f.client.propose(scope);await p.approval.onResolution('deny');await expect(p.grant()).rejects.toThrow('APPROVAL_DENIED');expect(f.bodies[1]?.decision).toBe('reject');expect(f.paths).toHaveLength(2);}finally{f.fetch.mockRestore();}
});

test('unqualified/missing native identity and unsafe destinations fail before networking',()=>{
 const options={qualified:true,owner:'owner',nativeToken:'synthetic-native-token',summaryMemoryEnabled:false};
 expect(()=>createNativeTaskClient({...options,qualified:false},{},undefined)).toThrow('TASK_TRANSPORT_UNQUALIFIED');
 expect(()=>createNativeTaskClient({...options,summaryMemoryEnabled:true},{},undefined)).toThrow('TASK_MEMORY_INTEGRATION_REQUIRED');
 expect(()=>createNativeTaskClient(options,{},new AbortController().signal)).toThrow('NATIVE_AUTHORITY_REQUIRED');
 const context={agentId:'a',sessionKey:'s',sessionId:'i',requesterSenderId:'owner',senderIsOwner:true,assertInvocationCurrent(){}};
 expect(()=>createNativeTaskClient({...options,coreUrl:'https://example.com'},context,new AbortController().signal)).toThrow('INVALID_CORE_DESTINATION');
 expect(()=>createNativeTaskClient({...options,nativeToken:''},context,new AbortController().signal)).toThrow('INVALID_CONFIGURATION');
 expect(()=>createNativeTaskClient(options,{...context,sessionId:''},new AbortController().signal)).toThrow('NATIVE_AUTHORITY_REQUIRED');
 expect(()=>createNativeTaskClient(options,context,undefined)).toThrow('NATIVE_AUTHORITY_REQUIRED');
});

test('authority revoked while Core prepares prevents the ready confirmation',async()=>{
 const f=fixture();try{
  const transport=f.fetch;transport.mockImplementation(async(input,init)=>{
   const request=new Request(input,init),member=await request.json();f.paths.push(new URL(request.url).pathname);f.revoke();
   return new Response(JSON.stringify({type:'ready',owner:'owner',challenge:{member,preview_digest:digest,nonce}})+'\n',{headers:{'content-type':'application/vnd.ciel.task-dispatch+ndjson;version=1'}});
  });
  await expect(f.client.execute(id,1,'op')).rejects.toThrow('INVOCATION_EXPIRED');expect(f.paths).toEqual(['/v1/write-tasks/execute']);
 }finally{f.fetch.mockRestore();}
});

test('wrong challenge and interrupted results never resend a member',async()=>{
 const f=fixture();try{
  f.response(new Response(JSON.stringify({type:'ready',owner:'owner',challenge:{member:{task_id:'other'},preview_digest:digest,nonce}})+'\n',{headers:{'content-type':'application/vnd.ciel.task-dispatch+ndjson;version=1'}}));
  await expect(f.client.execute(id,1,'op')).rejects.toThrow('INVALID_TASK_RESPONSE');expect(f.paths).toHaveLength(1);
  f.response(new Response('',{headers:{'content-type':'application/vnd.ciel.task-dispatch+ndjson;version=1'}}));
  await expect(f.client.execute(id,1,'op')).rejects.toThrow('TASK_OUTCOME_UNKNOWN');expect(f.paths).toHaveLength(2);
 }finally{f.fetch.mockRestore();}
});

test('a known preflight conflict needs no dispatch confirmation and preserves its outcome',async()=>{
 const f=fixture();try{f.response(new Response(JSON.stringify({type:'result',owner:'owner',ok:true,data:{state:'conflict',operation_id:'op'}})+'\n',{headers:{'content-type':'application/vnd.ciel.task-dispatch+ndjson;version=1'}}));expect(await f.client.execute(id,1,'op')).toEqual({type:'result',ok:true,data:{state:'conflict',operation_id:'op'}});expect(f.paths).toHaveLength(1);}finally{f.fetch.mockRestore();}
});

test('chunked Unicode result framing waits for the exact confirmation and rejects duplicate ready',async()=>{
 const f=fixture();try{
  let duplicate=false;
  f.fetch.mockImplementation(async(input,init)=>{
   const request=new Request(input,init),path=new URL(request.url).pathname;f.paths.push(path);
   if(path.endsWith('/confirm'))return Response.json({ok:true,owner:'owner',data:{confirmed:true}});
   const member=await request.json(),ready=JSON.stringify({type:'ready',owner:'owner',challenge:{member,preview_digest:digest,nonce}})+'\n';
   const bytes=new TextEncoder().encode(ready+(duplicate?ready:JSON.stringify({type:'result',owner:'owner',ok:true,data:{state:'applied',operation_id:'op',preview_digest:digest,text:'é🙂'}})+'\n'));
   return new Response(new ReadableStream({start(controller){for(let offset=0;offset<bytes.length;offset+=7)controller.enqueue(bytes.slice(offset,offset+7));controller.close();}}),{headers:{'content-type':'application/vnd.ciel.task-dispatch+ndjson;version=1'}});
  });
  expect(await f.client.execute(id,1,'op')).toEqual({type:'result',ok:true,data:{state:'applied',operation_id:'op',preview_digest:digest,text:'é🙂'}});
  duplicate=true;await expect(f.client.execute(id,1,'op')).rejects.toThrow('INVALID_TASK_RESPONSE');
  expect(f.paths.filter(path=>path.endsWith('/execute'))).toHaveLength(2);expect(f.paths.filter(path=>path.endsWith('/confirm'))).toHaveLength(2);
 }finally{f.fetch.mockRestore();}
});

test('malformed grant/confirmation and oversized responses cannot become authority',async()=>{
 const f=fixture();try{
  f.response(Response.json({ok:true,owner:'owner',data:{task_id:id,scope_digest:digest,scope,phase:'active',grant_revision:0}}));
  await expect(f.client.propose(scope)).rejects.toThrow('INVALID_TASK_RESPONSE');
  f.response(new Response('x'.repeat(524289),{headers:{'content-type':'application/json'}}));
  await expect(f.client.propose(scope)).rejects.toThrow('RESPONSE_TOO_LARGE');
  f.response(Response.json({ok:true,owner:'owner',data:{task_id:id,scope_digest:digest,scope:{...scope,objective:'wider'},phase:'proposed',grant_revision:0}}));
  await expect(f.client.propose(scope)).rejects.toThrow('INVALID_TASK_RESPONSE');
 }finally{f.fetch.mockRestore();}
});

test('changed owner and expired authority during encoding stop before the next request',async()=>{
 const f=fixture();try{
  const change={toJSON(){f.revoke();return {};}};
  await expect(f.client.capture(id,change)).rejects.toThrow('INVOCATION_EXPIRED');expect(f.paths).toHaveLength(0);
 }finally{f.fetch.mockRestore();}
 const second=fixture();try{second.disown();await expect(second.client.end(id,1,'completed')).rejects.toThrow('OWNER_REQUIRED');expect(second.paths).toHaveLength(0);}finally{second.fetch.mockRestore();}
});


test('unsupported native decisions never call Core approval and a failed acknowledgement grants nothing',async()=>{
 for(const decision of ['allow-always','timeout','cancelled'] as const) {
  const f=fixture();try{const p=await f.client.propose(scope);await expect(p.approval.onResolution(decision)).rejects.toThrow('APPROVAL_DENIED');await expect(p.grant()).rejects.toThrow('APPROVAL_DENIED');expect(f.paths).toHaveLength(1);}finally{f.fetch.mockRestore();}
 }
 const f=fixture();try{const p=await f.client.propose(scope);f.response(Response.json({ok:false,error:{code:'stale'}},{status:409}));await expect(p.approval.onResolution('allow-once')).rejects.toThrow('TASK_CORE_REJECTED');await expect(p.grant()).rejects.toThrow('TASK_CORE_REJECTED');expect(f.paths).toHaveLength(2);}finally{f.fetch.mockRestore();}
});


test('recreating a client cannot send a member before a delayed Core grant acknowledgement',async()=>{
 const f=fixture();let release:((value:Uint8Array)=>void)|undefined,close:(()=>void)|undefined;
 try{
  const proposal=await f.client.propose(scope);
  const body=new ReadableStream<Uint8Array>({start(controller){release=value=>controller.enqueue(value);close=()=>controller.close();}});
  f.response(new Response(body,{headers:{'content-type':'application/json'}}));
  const resolution=proposal.approval.onResolution('allow-once');
  let finished=false;const execution=f.recreate().execute(id,1,'op').then(result=>{finished=true;return result;});
  await new Promise(resolve=>setTimeout(resolve,5));expect(finished).toBe(false);expect(f.paths).toEqual(['/v1/write-tasks/native','/v1/write-tasks/native']);
  f.response(undefined);if(release===undefined || close===undefined)throw Error('Missing response controller');
  release(new TextEncoder().encode(JSON.stringify({ok:true,owner:'owner',data:{task_id:id,scope_digest:digest,grant_revision:1,phase:'active'}})));close();
  await resolution;expect((await execution)).toMatchObject({ok:true,data:{state:'applied'}});expect(f.paths.filter(path=>path.endsWith('/execute'))).toHaveLength(1);
 }finally{f.fetch.mockRestore();}
});

test('a lost grant acknowledgement requires authoritative inspection without resending the decision',async()=>{
 const f=fixture();try{
  const proposal=await f.client.propose(scope);f.response(new Response('not JSON'));
  await expect(proposal.approval.onResolution('allow-once')).rejects.toThrow('INVALID_TASK_RESPONSE');
  await expect(f.recreate().execute(id,1,'op')).rejects.toThrow('INVALID_TASK_RESPONSE');expect(f.paths).toHaveLength(2);
  f.response(Response.json({ok:true,owner:'owner',data:{task_id:id,scope_digest:digest,grant_revision:1,phase:'active',requires_confirmation:false}}));
  expect(await f.recreate().inspect(proposal.taskId)).toMatchObject({phase:'active',grant_revision:1});
  f.response(undefined);expect(await f.recreate().execute(id,1,'op')).toMatchObject({ok:true,data:{state:'applied'}});
  expect(f.bodies.filter(body=>body.action==='decide')).toHaveLength(1);
 }finally{f.fetch.mockRestore();}
});

test('a differently configured Core owner cannot issue a native grant or confirm an effect',async()=>{
 const f=fixture();try{
  f.response(Response.json({ok:true,owner:'other-owner',data:{task_id:id,scope,scope_digest:digest,grant_revision:0,phase:'proposed'}}));
  await expect(f.client.propose(scope)).rejects.toThrow('CORE_OWNER_MISMATCH');expect(f.paths).toHaveLength(1);
  f.fetch.mockImplementation(async(input,init)=>{const request=new Request(input,init),member=await request.json();f.paths.push(new URL(request.url).pathname);return new Response(JSON.stringify({type:'ready',owner:'other-owner',challenge:{member,preview_digest:digest,nonce}})+'\n',{headers:{'content-type':'application/vnd.ciel.task-dispatch+ndjson;version=1'}});});
  await expect(f.client.execute(id,1,'op')).rejects.toThrow('CORE_OWNER_MISMATCH');expect(f.paths).toHaveLength(2);
 }finally{f.fetch.mockRestore();}
});
