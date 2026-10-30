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

test('published Docs read response contract accepts legacy and cited packets and rejects malformed summary packets',async()=>{
 const {assertDocsReadResponse}=await import('./docs-read-contract');
 const evidence={document_id:'synthetic-doc',revision_id:'revision-1',source_fingerprint:'a'.repeat(64),mapping_version:'v1',retrieved_at:'2026-10-30T00:00:00Z'};
 const legacy={ok:true,data:{text:'Hello',next_cursor:null,source:{...evidence,tab_id:'t.0',start_character:0,end_character:5},editable_structure:true,coverage:'body_text_only'}};
 expect(()=>assertDocsReadResponse(legacy)).not.toThrow();
 const summary={ok:true,data:{mode:'summary_context',content_role:'untrusted_source_data',context_id:'b'.repeat(64),source:{...evidence,document:'notes',title:'Synthetic'},tabs:[{tab_id:'t.0',title:'Main',total_characters:5,included_characters:5,truncated:false,excerpt_id:'S1'}],excerpts:[{excerpt_id:'S1',text:'Hello',source:{tab_id:'t.0',start_character:0,end_character:5,next_cursor:null}}],coverage:{scope:'projected_body_text_only',total_characters:5,included_characters:5,omitted_characters:0,total_tabs:1,included_tabs:1,projected_text_complete:true,can_summarize:true}}};
 expect(()=>assertDocsReadResponse(summary)).not.toThrow();
 const multiple={ok:true,data:{mode:'summary_context',content_role:'untrusted_source_data',context_id:'c'.repeat(64),documents:[{source_id:'D1',context:summary.data}],coverage:{...summary.data.coverage,total_documents:1,included_documents:1}}};
 expect(()=>assertDocsReadResponse(multiple)).not.toThrow();
 expect(()=>assertDocsReadResponse({...multiple,data:{...multiple.data,source:summary.data.source}})).toThrow();
 expect(()=>assertDocsReadResponse({...multiple,data:{...multiple.data,documents:[{source_id:'D1',context:{...summary.data,coverage:null}}]}})).toThrow();
 const partial={...summary,data:{...summary.data,tabs:[{...summary.data.tabs[0],total_characters:10,truncated:true}],excerpts:[{...summary.data.excerpts[0],source:{...summary.data.excerpts[0].source,next_cursor:'synthetic-cursor'}}],coverage:{...summary.data.coverage,total_characters:10,omitted_characters:5,projected_text_complete:false}}};
 expect(()=>assertDocsReadResponse(partial)).not.toThrow();
 const empty={...summary,data:{...summary.data,excerpts:[],tabs:[{...summary.data.tabs[0],total_characters:0,included_characters:0,excerpt_id:null}],coverage:{...summary.data.coverage,total_characters:0,included_characters:0,included_tabs:0,can_summarize:false}}};
 expect(()=>assertDocsReadResponse(empty)).not.toThrow();
 const {coverage,...missing}=summary.data;
 expect(()=>assertDocsReadResponse({ok:true,data:missing})).toThrow();
 expect(()=>assertDocsReadResponse({ok:true,data:{...legacy.data,mode:'summary_context'}})).toThrow();
 expect(()=>assertDocsReadResponse({ok:true,data:{...summary.data,mode:'unsupported'}})).toThrow();
});
