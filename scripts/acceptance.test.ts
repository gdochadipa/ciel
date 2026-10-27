import {test,expect} from 'bun:test';
import {acceptanceConfiguration,simulationGoogleBase} from './acceptance';
import {createSecretary} from '../plugins/secretary-tools/src/bridge';
test('acceptance refuses production or arbitrary Core destinations before starting traffic',()=>{
 expect(()=>acceptanceConfiguration('https://sheets.googleapis.com','read')).toThrow('disposable Core');
 expect(()=>acceptanceConfiguration('http://127.0.0.1:8787/redirect','read')).toThrow('disposable Core');
 expect(()=>acceptanceConfiguration('http://127.0.0.1:8787','write-unreviewed')).toThrow('Unsupported');
 expect(acceptanceConfiguration('http://127.0.0.1:8787','read').coreToken).toBe('synthetic-acceptance-core-token');
});
test('public reminder history keeps execution completion and channel evidence independent',async()=>{
 const bridge=createSecretary({coreUrl:'http://127.0.0.1:1',coreToken:'synthetic',owner:'owner',schedulerQualified:true,gateway:{isAvailable:()=>true,async request(method):Promise<any>{
  if(method==='cron.list') return {jobs:[{id:'native-42',declarationKey:'secretary:owner:deposit'}]};
  if(method==='cron.runs') return {entries:[{runId:'run-1',status:'ok',completionStatus:'completed',delivered:false,deliveryStatus:'not-delivered'},{runId:'run-2',status:'error',completionStatus:'failed',delivered:true,deliveryStatus:'delivered'}]};
  throw Error('Unsupported fixture route');
 }}});
 expect(await bridge.execute('reminders_history',{reminder_id:'deposit'},{requesterSenderId:'owner',senderIsOwner:true})).toEqual({entries:[{runId:'run-1',status:'ok',completionStatus:'completed',delivered:false,deliveryStatus:'not-delivered'},{runId:'run-2',status:'error',completionStatus:'failed',delivered:true,deliveryStatus:'delivered'}]});
});

test('simulation wiring derives only an observed loopback URL',()=>{
 expect(simulationGoogleBase('{"event":"listening","members":[{"url":"http://127.0.0.1:54321/google"}]}')).toBe('http://127.0.0.1:54321/google/v4');
 expect(()=>simulationGoogleBase('{"event":"listening","url":"https://sheets.googleapis.com"}')).toThrow('Listening event');
});

test('workspace wiring requires an observed Docs member address',()=>{
 const log='{"event":"listening","mocks":[{"address":"http://localhost:4900/google"},{"address":"http://localhost:4900/google-docs"}]}';
 expect(simulationGoogleBase(log,'google-docs','v1')).toBe('http://127.0.0.1:4900/google-docs/v1');
 expect(()=>simulationGoogleBase('{"event":"listening","url":"http://localhost:4900"}','google-docs','v1')).toThrow('Listening event');
 expect(acceptanceConfiguration('http://127.0.0.1:8787','workspace').owner).toBe('+15555550123');
});
