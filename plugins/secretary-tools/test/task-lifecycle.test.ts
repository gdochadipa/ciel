import {test,expect} from 'bun:test';
import {TaskSessionRetirements} from '../src/task-lifecycle';
import type {NativeTaskRevoker} from '../src/task-native';
function fixture(sessionId='old',connection='core'){
 let current=true,calls=0,rejected=false,release:(()=>void)|undefined;
 const identity={agentId:'agent',sessionKey:'owner-session',sessionId};let gate:Promise<void>|undefined;
 const captured:NativeTaskRevoker={identity,connection,check(){if(!current)throw Error('INVOCATION_EXPIRED');},async revoke(){calls++;if(gate)await gate;if(rejected)throw Error('NETWORK_OUTCOME_UNKNOWN');}};
 return {captured,calls:()=>calls,expire(){current=false;},refuse(value:boolean){rejected=value;},hold(){gate=new Promise<void>(done=>{release=done;});},release(){if(release===undefined)throw Error('MISSING_GATE');release();}};
}
test('ordinary turn completion retains enrollment; session end revokes its captured identity even after the run expires',async()=>{
 const state=new TaskSessionRetirements(),old=fixture();state.enroll(old.captured);old.expire();
 await state.wait(fixture().captured);expect(old.calls()).toBe(0);
 await state.end(old.captured.identity);expect(old.calls()).toBe(1);
 await state.end(old.captured.identity);expect(old.calls()).toBe(1);
});
test('a successor waits for the exact old Core acknowledgement and duplicate hooks share one request',async()=>{
 const state=new TaskSessionRetirements(),old=fixture(),next=fixture('next');state.enroll(old.captured);old.hold();
 const end=state.end(old.captured.identity),duplicate=state.end(old.captured.identity);let continued=false;
 const continuation=state.wait(next.captured).then(()=>{continued=true;});await Promise.resolve();expect(continued).toBe(false);expect(old.calls()).toBe(1);
 old.release();await Promise.all([end,duplicate,continuation]);expect(continued).toBe(true);expect(old.calls()).toBe(1);
});
test('a failed metadata acknowledgement blocks continuation; the next admitted call retries revocation without a provider effect',async()=>{
 const state=new TaskSessionRetirements(),old=fixture(),next=fixture('next');state.enroll(old.captured);old.refuse(true);
 await expect(state.end(old.captured.identity)).rejects.toThrow('TASK_LIFECYCLE_RECONCILIATION_REQUIRED');
 await expect(state.wait(next.captured)).rejects.toThrow('TASK_LIFECYCLE_RECONCILIATION_REQUIRED');expect(old.calls()).toBe(2);
 old.refuse(false);await state.wait(next.captured);expect(old.calls()).toBe(3);await state.wait(next.captured);expect(old.calls()).toBe(3);
});
test('incomplete or foreign end notifications never select an enrolled context or its successor',async()=>{
 const state=new TaskSessionRetirements(),old=fixture(),next=fixture('next');state.enroll(old.captured);state.enroll(next.captured);
 for(const identity of [{agentId:'agent',sessionKey:'owner-session'},{...old.captured.identity,agentId:'other'},{...old.captured.identity,sessionKey:'other'}])await state.end(identity);
 expect(old.calls()).toBe(0);expect(next.calls()).toBe(0);await state.end(old.captured.identity);expect(old.calls()).toBe(1);expect(next.calls()).toBe(0);
 await state.end(old.captured.identity);expect(next.calls()).toBe(0);
});
test('pending revocation in another connection cannot block a different Core; revoked currentness still blocks the matching continuation',async()=>{
 const state=new TaskSessionRetirements(),old=fixture();state.enroll(old.captured);old.refuse(true);await expect(state.end(old.captured.identity)).rejects.toThrow('TASK_LIFECYCLE_RECONCILIATION_REQUIRED');
 const other=fixture('next','different-core');await state.wait(other.captured);expect(old.calls()).toBe(1);
 old.refuse(false);const expired=fixture('next');expired.expire();await expect(state.wait(expired.captured)).rejects.toThrow('INVOCATION_EXPIRED');expect(old.calls()).toBe(2);
});
test('bounded enrollment retains all live identities instead of evicting an old revocation capability',async()=>{
 const state=new TaskSessionRetirements(),first=fixture('0');state.enroll(first.captured);state.enroll(first.captured);
 for(let i=1;i<64;i++)state.enroll(fixture(String(i)).captured);
 expect(()=>state.enroll(fixture('overflow').captured)).toThrow('TASK_LIFECYCLE_CAPACITY');
 await state.end(first.captured.identity);expect(first.calls()).toBe(1);state.enroll(fixture('overflow').captured);
});
