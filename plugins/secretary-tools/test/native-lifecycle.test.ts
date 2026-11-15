import {test,expect} from 'bun:test';
import {NativeSessionLifecycle} from '../src/native-lifecycle';

function host(sessionId='old',sessionKey='owner',agentId='agent') {
 let current=true,owner=true;
 return {context:{agentId,sessionKey,sessionId,get senderIsOwner(){return owner;},assertInvocationCurrent(){if(!current)throw Error('HOST_EXPIRED');}},expire(){current=false;},disown(){owner=false;}};
}

test('ended incarnation revokes retained and newly bound callbacks without revoking its successor',()=>{
 const lifecycle=new NativeSessionLifecycle(),old=host(),next=host('next');
 const retained=lifecycle.bind(old.context),replacement=lifecycle.bind(next.context);
 retained.assertInvocationCurrent();lifecycle.end(old.context);
 expect(()=>retained.assertInvocationCurrent()).toThrow('INVOCATION_EXPIRED');
 expect(()=>lifecycle.bind(old.context).assertInvocationCurrent()).toThrow('INVOCATION_EXPIRED');
 replacement.assertInvocationCurrent();lifecycle.end(old.context);replacement.assertInvocationCurrent();
});

test('lifecycle joins exact agent, logical conversation and incarnation; incomplete hook data grants nothing',()=>{
 const lifecycle=new NativeSessionLifecycle(),h=host(),bound=lifecycle.bind(h.context);
 for(const identity of [{...h.context,agentId:'other'},{...h.context,sessionKey:'other'},{...h.context,sessionId:'other'},{agentId:'agent',sessionKey:'owner'},{...h.context,sessionId:''}])lifecycle.end(identity);
 bound.assertInvocationCurrent();lifecycle.end(h.context);
 expect(()=>bound.assertInvocationCurrent()).toThrow('INVOCATION_EXPIRED');
});

test('native assertion, cancellation and live owner predicate remain authoritative',()=>{
 const lifecycle=new NativeSessionLifecycle(),h=host(),abort=new AbortController(),bound=lifecycle.bind(h.context,abort.signal);
 expect(bound.senderIsOwner).toBe(true);h.disown();expect(bound.senderIsOwner).toBe(false);
 abort.abort();expect(()=>bound.assertInvocationCurrent()).toThrow('INVOCATION_EXPIRED');
 const other=host('other'),retained=lifecycle.bind(other.context);other.expire();
 expect(()=>retained.assertInvocationCurrent()).toThrow('HOST_EXPIRED');
 expect(()=>lifecycle.bind({senderIsOwner:true})).toThrow('NATIVE_AUTHORITY_REQUIRED');
});

test('revocation capacity never evicts a closed incarnation; saturation denies further bindings',()=>{
 const lifecycle=new NativeSessionLifecycle(),first=host('0'),retained=lifecycle.bind(host('active').context);
 for(let index=0;index<4096;index++)lifecycle.end(host(String(index)).context);
 expect(()=>lifecycle.bind(first.context).assertInvocationCurrent()).toThrow('INVOCATION_EXPIRED');
 lifecycle.end(first.context);retained.assertInvocationCurrent();
 lifecycle.end(host('overflow').context);
 expect(()=>retained.assertInvocationCurrent()).toThrow('NATIVE_LIFECYCLE_CAPACITY');
 expect(()=>lifecycle.bind(host('new').context).assertInvocationCurrent()).toThrow('NATIVE_LIFECYCLE_CAPACITY');
});
