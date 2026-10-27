/** Real bridge/Core -> simulated Google Sheets and Docs, never fake Core/HTTP. */
import assert from 'node:assert/strict';
import {createSecretary} from '../plugins/secretary-tools/src/bridge';
import {acceptanceConfiguration,syntheticOwner,syntheticToken} from './acceptance';
export async function runWorkspaceAcceptance(coreUrl:string,docsUrl:string) {
 const base=acceptanceConfiguration(coreUrl,'workspace');const url=new URL(docsUrl);
 assert.equal(url.protocol,'http:');assert.equal(url.hostname,'127.0.0.1');assert.equal(url.pathname,'/google-docs/v1');assert(!url.username&&!url.password&&!url.search&&!url.hash);
 const operatorToken='synthetic-separate-operator-token';
 const bridge=createSecretary({...base,approvalToken:operatorToken,approvalTransportQualified:true});
 const owner={requesterSenderId:syntheticOwner,senderIsOwner:true};
 const tool=async(name:string,input:unknown)=>await bridge.execute(name,input,owner) as any;
 async function fixture(path:string,input?:unknown){
  const result=await fetch(docsUrl.replace(/\/v1$/,'')+path,{method:input===undefined?'GET':'POST',headers:{'content-type':'application/json'},body:input===undefined?undefined:JSON.stringify(input),signal:AbortSignal.timeout(5000)});assert(result.ok);return result.json() as any;
 }
 async function operator(id:string,action:string,input:unknown,token=operatorToken){
  const response=await fetch(`${coreUrl}/v1/operations/${id}/${action}`,{method:'POST',headers:{authorization:`Bearer ${token}`,'content-type':'application/json'},body:JSON.stringify(input),signal:AbortSignal.timeout(5000)});return response.json() as any;
 }
 let ready=false;for(let attempt=0;attempt<50;attempt++){
  try{if((await tool('docs_list',{})).ok){ready=true;break;}}catch{}await Bun.sleep(200);
 }assert(ready,'Actual Core did not become ready');
 await fixture('/__fixture/reset',{});
 const grid=await tool('sheets_read',{spreadsheet:'operations',view:'overview',limit:2});
 assert.equal(grid.ok,true);assert.deepEqual(grid.data.records[0].fields,{B:'Independent',C:12,D:true});assert.equal(grid.data.records[1].fields.C,6);
 assert.equal((await tool('sheets_search',{spreadsheet:'operations',view:'overview',query:'true'})).data.records.length,1);
 assert.equal((await tool('reports_wedding',{spreadsheet:'operations'})).ok,false,'Domain profile must not be required for generic reads');
 const list=await tool('docs_list',{});assert.equal(list.data.documents[0].document,'notes');
 assert.equal((await tool('docs_describe',{document:'not-registered'})).error.code,'document_not_registered');
 await assert.rejects(bridge.execute('docs_read',{document:'notes',tab_id:'t.0'},{}),/OWNER_REQUIRED/);
 const described=await tool('docs_describe',{document:'notes'});assert.equal(described.data.tabs.length,2);
 const first=await tool('docs_read',{document:'notes',tab_id:'t.0',limit:2});assert.equal(first.data.text,'He');assert(first.data.next_cursor);
 const next=await tool('docs_read',{document:'notes',tab_id:'t.0',limit:2,cursor:first.data.next_cursor});assert.equal(next.data.text,'ll');
 const change={operation_id:'docs-append-one',document:'notes',tab_id:'t.0',action:'append_text',fields:{text:' Added'}};
 const preview=await tool('docs_preview_change',change);assert.equal(preview.ok,true);assert.equal(preview.data.resource_kind,'google_docs');assert.equal(preview.data.before.text,'Hello sample\n');assert.equal(preview.data.after.text,'Hello sample Added\n');
 assert.equal((await tool('docs_apply',{operation_id:change.operation_id})).ok,false,'Unapproved intent cannot execute');
 assert.equal((await operator(change.operation_id,'approval',{decision:'approve',preview_digest:preview.data.preview_digest},syntheticToken)).ok,false);
 assert.equal((await operator(change.operation_id,'approval',{decision:'approve',preview_digest:'wrong'})).ok,false);
 const approval=(await bridge.prepareApproval('docs_apply',{operation_id:change.operation_id},owner))!;
 assert(approval.description.includes(JSON.stringify(preview.data,null,2)));await approval.onResolution('allow-once');
 await assert.rejects(approval.onResolution('allow-once'),/APPROVAL_ALREADY_RESOLVED/);
 assert.equal((await tool('docs_apply',{operation_id:change.operation_id})).data.state,'applied');
 assert.equal((await tool('docs_apply',{operation_id:change.operation_id})).data.state,'applied');
 assert.equal((await fixture('/__fixture/stats')).applies,1);
 assert.equal((await tool('docs_read',{document:'notes',tab_id:'t.1'})).data.text,'Untouched tab\n');
 assert.equal((await tool('docs_read',{document:'notes',tab_id:'t.0',cursor:first.data.next_cursor})).error.code,'stale_cursor');
 const replace={...change,operation_id:'docs-replace-one',action:'replace_text',fields:{find:'sample',replacement:'team'}};
 const replaced=await tool('docs_preview_change',replace);assert.equal(replaced.ok,true);
 await operator(replace.operation_id,'approval',{decision:'approve',preview_digest:replaced.data.preview_digest});
 assert.equal((await tool('docs_apply',{operation_id:replace.operation_id})).data.state,'applied');
 assert.equal((await tool('docs_read',{document:'notes',tab_id:'t.0'})).data.text,'Hello team Added\n');
 const stale={...change,operation_id:'docs-stale'};const stalePreview=await tool('docs_preview_change',stale);
 await operator(stale.operation_id,'approval',{decision:'approve',preview_digest:stalePreview.data.preview_digest});
 await fixture('/__fixture/external-edit',{text:'External editor\n'});
 assert.equal((await tool('docs_apply',{operation_id:stale.operation_id})).data.state,'conflict');
 assert.equal((await fixture('/__fixture/stats')).applies,2);
 const lost={...change,operation_id:'docs-lost'};const lostPreview=await tool('docs_preview_change',lost);
 await operator(lost.operation_id,'approval',{decision:'approve',preview_digest:lostPreview.data.preview_digest});
 await fixture('/__fixture/lose-next-response',{});
 assert.equal((await tool('docs_apply',{operation_id:lost.operation_id})).data.state,'uncertain');
 assert.equal((await tool('docs_apply',{operation_id:lost.operation_id})).ok,false);
 assert.equal((await operator(lost.operation_id,'reconcile',{})).data.state,'applied');
 assert.equal((await fixture('/__fixture/stats')).applies,3);
 const history=await tool('operations_get_history',{operation_id:lost.operation_id});assert.equal(history.data.operation.state,'applied');assert.equal(history.data.operation.document,'notes');
 const {runEntryAcceptance}=await import('./acceptance-entry');await runEntryAcceptance(coreUrl,'docs');
 assert.equal((await fixture('/__fixture/stats')).applies,4);
 console.log(JSON.stringify({acceptance:'passed',scenario:'workspace',scope:'generic Sheets + controlled Docs through actual SDK/bridge/Core; only Google simulated, no live Docs authorization'}));
}
