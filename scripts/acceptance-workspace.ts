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
 const targets=[{spreadsheet:'operations',view:'overview'},{spreadsheet:'archive',view:'overview'}];
 const cross=await tool('sheets_search',{targets,query:'INDEPENDENT',limit:1});assert.equal(cross.ok,true);assert.equal(cross.data.sources.length,2);assert.equal(cross.data.records[0].spreadsheet,'operations');assert(cross.data.next_cursor);
 const crossNext=await tool('sheets_search',{targets,query:'INDEPENDENT',limit:1,cursor:cross.data.next_cursor});assert.equal(crossNext.data.records[0].spreadsheet,'archive');assert.equal(crossNext.data.records[0].fields.C,5);assert.equal(crossNext.data.next_cursor,null);
 assert.equal((await tool('sheets_search',{targets:targets.toReversed(),query:'INDEPENDENT',cursor:cross.data.next_cursor})).error.code,'stale_cursor');
 assert.equal((await tool('reports_wedding',{spreadsheet:'operations'})).ok,false,'Domain profile must not be required for generic reads');
 const list=await tool('docs_list',{});assert.equal(list.data.documents[0].document,'notes');
 assert.equal((await tool('docs_describe',{document:'not-registered'})).error.code,'document_not_registered');
 await assert.rejects(bridge.execute('docs_read',{document:'notes',tab_id:'t.0'},{}),/OWNER_REQUIRED/);
 const described=await tool('docs_describe',{document:'notes'});assert.equal(described.data.tabs.length,2);
 const context=await tool('docs_read',{document:'notes',mode:'summary_context',max_characters:100});assert.equal(context.ok,true);assert.equal(context.data.excerpts[0].excerpt_id,'S1');assert.equal(context.data.excerpts[0].text,'Hello sample\n');assert.equal(context.data.coverage.projected_text_complete,true);assert.equal(context.data.source.document_id,'demo-document');assert.equal(context.data.content_role,'untrusted_source_data');
 const first=await tool('docs_read',{document:'notes',tab_id:'t.0',limit:2});assert.equal(first.data.text,'He');assert(first.data.next_cursor);
 const next=await tool('docs_read',{document:'notes',tab_id:'t.0',limit:2,cursor:first.data.next_cursor});assert.equal(next.data.text,'ll');
 const found=await tool('docs_search',{document:'notes',query:'e',limit:1});assert.equal(found.ok,true);assert(found.data.next_cursor);assert.equal(found.data.matches[0].tab_id,'t.0');
 const foundNext=await tool('docs_search',{document:'notes',query:'e',limit:1,cursor:found.data.next_cursor});assert.equal(foundNext.ok,true);
 assert.equal((await tool('docs_search',{document:'notes',query:'tab',tab_id:'t.1'})).data.matches.length,1);
 assert.equal((await tool('docs_search',{document:'notes',query:'absent phrase'})).data.matches.length,0);
 const insensitive=await tool('docs_search',{document:'notes',query:'HELLO',case_sensitive:false});assert.equal(insensitive.data.matches.length,1);assert.equal(insensitive.data.matches[0].text,'Hello sample\n');assert.equal(insensitive.data.match_mode,'literal_case_insensitive');
 assert.equal((await tool('docs_search',{document:'notes',query:'HELLO'})).data.matches.length,0);
 const many=await tool('docs_search',{documents:['notes','reference'],query:'HELLO',case_sensitive:false,limit:1});assert.equal(many.ok,true);assert.equal(many.data.sources.length,2);assert.equal(many.data.matches[0].document,'notes');assert(many.data.next_cursor);
 const manyNext=await tool('docs_search',{documents:['notes','reference'],query:'HELLO',case_sensitive:false,limit:1,cursor:many.data.next_cursor});assert.equal(manyNext.data.matches[0].document,'reference');assert.equal(manyNext.data.next_cursor,null);
 assert.equal((await tool('docs_search',{documents:['reference','notes'],query:'HELLO',case_sensitive:false,cursor:many.data.next_cursor})).error.code,'stale_cursor');
 const mixedPage=await tool('docs_search',{document:'notes',query:'E',case_sensitive:false,limit:1});assert(mixedPage.data.next_cursor);
 assert.equal((await tool('docs_search',{document:'notes',query:'E',cursor:mixedPage.data.next_cursor})).error.code,'stale_cursor');
 assert.equal((await fixture('/__fixture/stats')).applies,0);
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
 assert.equal((await tool('docs_search',{document:'notes',query:'e',cursor:found.data.next_cursor})).error.code,'stale_cursor');
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
 // Same real bridge/Core/journal for bounded new tabs. Only Google is simulated.
 const sheetsFixture=docsUrl.replace('/google-docs/v1','/google');
 async function sheetFixture(path:string,post=false){const r=await fetch(sheetsFixture+path,{method:post?'POST':'GET'});assert(r.ok);return r.json() as any;}
 const tab={operation_id:'tab-create-one',spreadsheet:'operations',title:'Team plan',rows:20,columns:3,headers:['Task','=literal']};
 const tabPreview=await tool('sheets_preview_create_tab',tab);assert.equal(tabPreview.ok,true);assert.equal(tabPreview.data.after.registration,'unregistered');assert.equal(tabPreview.data.after.readable,false);assert.equal(tabPreview.data.after.writable,false);
 assert.deepEqual((await tool('sheets_preview_create_tab',tab)).data,tabPreview.data);
 assert.equal((await tool('sheets_create_tab',{operation_id:tab.operation_id})).ok,false);
 const tabApproval=(await bridge.prepareApproval('sheets_create_tab',{operation_id:tab.operation_id},owner))!;assert(tabApproval.description.includes(JSON.stringify(tabPreview.data,null,2)));await tabApproval.onResolution('allow-once');
 assert.equal((await tool('sheets_create_tab',{operation_id:tab.operation_id})).data.state,'applied');assert.equal((await tool('sheets_create_tab',{operation_id:tab.operation_id})).data.state,'applied');
 assert.equal((await sheetFixture('/__fixture/stats')).creates,1);
 assert.equal((await tool('sheets_describe',{spreadsheet:'operations'})).data.views.length,1);
 assert.equal((await tool('sheets_preview_create_tab',{...tab,operation_id:'tab-collision'})).error.code,'tab_collision');
 const lostTab={...tab,operation_id:'tab-lost',title:'Second plan',headers:[]};const lostTabPreview=await tool('sheets_preview_create_tab',lostTab);assert.equal(lostTabPreview.ok,true);
 await operator(lostTab.operation_id,'approval',{decision:'approve',preview_digest:lostTabPreview.data.preview_digest});await sheetFixture('/__fixture/lose-next-response',true);
 assert.equal((await tool('sheets_create_tab',{operation_id:lostTab.operation_id})).data.state,'uncertain');assert.equal((await tool('sheets_create_tab',{operation_id:lostTab.operation_id})).ok,false);
 assert.equal((await operator(lostTab.operation_id,'reconcile',{})).data.state,'applied');assert.equal((await sheetFixture('/__fixture/stats')).creates,2);
 const gridChange={operation_id:'grid-set-one',spreadsheet:'operations',view:'overview',action:'update',fields:{range:'B4:C4',values:[['=literal',null]]}};
 const gridPreview=await tool('sheets_preview_change',gridChange);assert.equal(gridPreview.ok,true);assert.equal(gridPreview.data.resource_kind,'sheets_grid');
 const gridApproval=(await bridge.prepareApproval('sheets_update',{operation_id:gridChange.operation_id},owner))!;assert(gridApproval.description.includes(JSON.stringify(gridPreview.data,null,2)));await gridApproval.onResolution('allow-once');
 assert.equal((await tool('sheets_update',{operation_id:gridChange.operation_id})).data.state,'applied');assert.equal((await tool('sheets_update',{operation_id:gridChange.operation_id})).data.state,'applied');assert.equal((await sheetFixture('/__fixture/stats')).gridWrites,1);
 const lostGrid={...gridChange,operation_id:'grid-set-lost',fields:{range:'B4:C4',values:[[true,42]]}};const lostGridPreview=await tool('sheets_preview_change',lostGrid);assert.equal(lostGridPreview.ok,true);
 await operator(lostGrid.operation_id,'approval',{decision:'approve',preview_digest:lostGridPreview.data.preview_digest});await sheetFixture('/__fixture/lose-next-response',true);
 assert.equal((await tool('sheets_update',{operation_id:lostGrid.operation_id})).data.state,'uncertain');assert.equal((await tool('sheets_update',{operation_id:lostGrid.operation_id})).ok,false);assert.equal((await operator(lostGrid.operation_id,'reconcile',{})).data.state,'applied');assert.equal((await sheetFixture('/__fixture/stats')).gridWrites,2);
 const {runEntryAcceptance}=await import('./acceptance-entry');await runEntryAcceptance(coreUrl,'docs');await runEntryAcceptance(coreUrl,'tabs');await runEntryAcceptance(coreUrl,'grid');assert.equal((await sheetFixture('/__fixture/stats')).gridWrites,3);
 assert.equal((await fixture('/__fixture/stats')).applies,4);
 console.log(JSON.stringify({acceptance:'passed',scenario:'workspace',scope:'generic Sheets + controlled grid edits + tab creation + controlled Docs through actual SDK/bridge/Core; only Google simulated, no live Docs authorization'}));
}
