import {test,expect,spyOn} from 'bun:test';
import type {OpenClawPluginToolContext} from 'openclaw/plugin-sdk/plugin-entry';
import {bindNativeInvocation,NativeApprovalAuthority} from '../src/native-authority';
import {createSecretary} from '../src/bridge';

function host(sessionId='incarnation'): {context:OpenClawPluginToolContext<2>;revoke:()=>void;disown:()=>void} {
 let active=true,owner=true;
 return {context:{agentId:'agent',sessionKey:'agent:owner',sessionId,requesterSenderId:'owner',get senderIsOwner(){return owner;},assertInvocationCurrent(){if(!active)throw Error('INVOCATION_EXPIRED');}},revoke(){active=false;},disown(){owner=false;}};
}
function request(sessionId='incarnation') {
 return {agentId:'agent',sessionKey:'agent:owner',sessionId,requester:{senderId:'owner',senderIsOwner:true},abortSignal:new AbortController().signal};
}

test('native context requires a host closure and preserves its live owner getter',()=>{
 expect(()=>bindNativeInvocation({senderIsOwner:true})).toThrow('NATIVE_AUTHORITY_REQUIRED');
 const h=host(),context=bindNativeInvocation(h.context);
 expect(context.senderIsOwner).toBe(true);h.disown();expect(context.senderIsOwner).toBe(false);
 h.revoke();expect(()=>context.assertInvocationCurrent()).toThrow('INVOCATION_EXPIRED');
});

test('tool cancellation rejects a retained execution closure',()=>{
 const signal=new AbortController(),context=bindNativeInvocation(host().context,signal.signal);
 context.assertInvocationCurrent();signal.abort();
 expect(()=>context.assertInvocationCurrent()).toThrow('INVOCATION_EXPIRED');
});

test('approval binds exact conversation, tool and owner instead of borrowing input descriptors',()=>{
 const registry=new NativeApprovalAuthority(),h=host();registry.remember('docs_apply',h.context);
 for(const changed of [{agentId:'other'},{sessionKey:'other'},{sessionId:'new'}]) {
  expect(()=>registry.capture('docs_apply',{...request(),...changed})).toThrow('NATIVE_AUTHORITY_UNAVAILABLE');
 }
 expect(()=>registry.capture('sheets_update',request())).toThrow('NATIVE_AUTHORITY_UNAVAILABLE');
 for(const requester of [undefined,{senderId:'other',senderIsOwner:true},{senderId:'owner',senderIsOwner:false}]) {
  const captured=registry.capture('docs_apply',{...request(),requester});
  expect(()=>captured.assertInvocationCurrent()).toThrow('OWNER_REQUIRED');
 }
 registry.capture('docs_apply',request()).assertInvocationCurrent();
});

test('missing conversation fields or cancellation signal cannot create approval authority',()=>{
 const registry=new NativeApprovalAuthority();registry.remember('docs_apply',host().context);
 for(const changed of [{agentId:undefined},{sessionKey:''},{sessionId:undefined},{abortSignal:undefined},{sessionId:'x'.repeat(513)}]) {
  expect(()=>registry.capture('docs_apply',{...request(),...changed})).toThrow('NATIVE_AUTHORITY_REQUIRED');
 }
 const incomplete=new NativeApprovalAuthority();
 incomplete.remember('docs_apply',{...host().context,sessionId:undefined});
 expect(()=>incomplete.capture('docs_apply',request())).toThrow('NATIVE_AUTHORITY_UNAVAILABLE');
});

test('ambiguous live factories fail closed while exact repeated registration is idempotent',()=>{
 const registry=new NativeApprovalAuthority(),first=host(),second=host();
 registry.remember('docs_apply',first.context);registry.remember('docs_apply',first.context);
 registry.capture('docs_apply',request()).assertInvocationCurrent();
 registry.remember('docs_apply',second.context);
 expect(()=>registry.capture('docs_apply',request())).toThrow('NATIVE_AUTHORITY_UNAVAILABLE');
 first.revoke();registry.capture('docs_apply',request()).assertInvocationCurrent();
});

test('reset never transfers a retained callback to a new incarnation',()=>{
 const registry=new NativeApprovalAuthority(),old=host();registry.remember('docs_apply',old.context);
 const retained=registry.capture('docs_apply',request());old.revoke();
 registry.remember('docs_apply',host('new').context);
 expect(()=>retained.assertInvocationCurrent()).toThrow('INVOCATION_EXPIRED');
 expect(()=>registry.capture('docs_apply',request())).toThrow('NATIVE_AUTHORITY_UNAVAILABLE');
 registry.capture('docs_apply',request('new')).assertInvocationCurrent();
});

test('approval cancellation and live owner loss invalidate already captured callbacks',()=>{
 const registry=new NativeApprovalAuthority(),h=host(),signal=new AbortController();registry.remember('docs_apply',h.context);
 const cancelled=registry.capture('docs_apply',{...request(),abortSignal:signal.signal});signal.abort();
 expect(()=>cancelled.assertInvocationCurrent()).toThrow('INVOCATION_EXPIRED');
 const retained=registry.capture('docs_apply',request());h.disown();
 expect(retained.senderIsOwner).toBe(false);
  expect(()=>retained.assertInvocationCurrent()).toThrow('OWNER_REQUIRED');
 expect(()=>registry.capture('docs_apply',request())).toThrow('NATIVE_AUTHORITY_UNAVAILABLE');
});

test('bounded authority lookup never evicts a live callback to admit another',()=>{
 const registry=new NativeApprovalAuthority();const hosts=Array.from({length:65},(_,index)=>host(String(index)));
 for(const h of hosts)registry.remember('docs_apply',h.context);
 registry.capture('docs_apply',request('0')).assertInvocationCurrent();
 expect(()=>registry.capture('docs_apply',request('64'))).toThrow('NATIVE_AUTHORITY_UNAVAILABLE');
 const first=hosts[0],last=hosts[64];expect(first).toBeDefined();expect(last).toBeDefined();
 if(first===undefined || last===undefined)throw Error('Missing test host');
 first.revoke();registry.remember('docs_apply',last.context);
 registry.capture('docs_apply',request('64')).assertInvocationCurrent();
});

test('authority lost during request encoding prevents even the first Core send',async()=>{
 const h=host();let sends=0;
 const fetch=spyOn(globalThis,'fetch').mockImplementation(async()=>{sends++;return Response.json({ok:true,data:{}});});
 try {
  const bridge=createSecretary({coreToken:'synthetic',approvalToken:'synthetic-approval',owner:'owner',approvalTransportQualified:true});
  const fields={toJSON(){h.revoke();return {range:'A1:A1',values:[['new']]};}};
  await expect(bridge.execute('sheets_preview_change',{operation_id:'op',spreadsheet:'sheet',view:'grid',action:'update',fields},h.context)).rejects.toThrow('INVOCATION_EXPIRED');
  expect(sends).toBe(0);
 } finally {fetch.mockRestore();}
});
