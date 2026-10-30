/** Built plugin + real SDK registration; the host registry is a boundary fixture.
 * Core, journal and HTTP execute for real. No live Gateway or approval UI is started.
 */
import assert from 'node:assert/strict';
import {getToolPluginMetadata} from 'openclaw/plugin-sdk/tool-plugin';
import type {AnyAgentTool,OpenClawPluginApi} from 'openclaw/plugin-sdk/plugin-entry';
import {acceptanceConfiguration,syntheticOwner,syntheticToken} from './acceptance';

export async function runEntryAcceptance(coreUrl:string,kind:'sheets'|'docs'|'tabs'|'grid'='sheets') {
 const base=acceptanceConfiguration(coreUrl,'writes');
 const saved={core:process.env.SECRETARY_CORE_TOKEN,approval:process.env.SECRETARY_APPROVAL_TOKEN};
 delete process.env.SECRETARY_CORE_TOKEN;delete process.env.SECRETARY_APPROVAL_TOKEN;
 try {
  // Load the built distribution entry, not a re-created factory or SDK declaration.
  const {default:entry}=await import('../plugins/secretary-tools/dist/index.js');
  const metadata=getToolPluginMetadata(entry);
  const manifest=await Bun.file(new URL('../plugins/secretary-tools/openclaw.plugin.json',import.meta.url)).json();
  assert(metadata,'Actual SDK must recognize metadata after the plugin extends register');
  assert.deepEqual(metadata.tools.map(tool=>tool.name).sort(),manifest.contracts.tools.toSorted());
  assert.equal(metadata.tools.length,26);
  let runtimeAllowed=false;
  function registry() {
   const tools=new Map<string,Function>();
   let hook:Function|undefined;
   // Only registration is represented here. Never substitute a host SDK/runtime.
   const api={
    pluginConfig:{owner:syntheticOwner,coreUrl:base.coreUrl,approvalTransportQualified:true,schedulerQualified:false},
    get runtime() {
     assert(runtimeAllowed,'Registration/factory construction must not access runtime services');
     return {gateway:{isAvailable:()=>false,request:()=>{throw Error('No Gateway is available in this fixture');}}};
    },
    registerTool(factory:unknown,options:{name:string}) {
     assert.equal(typeof factory,'function');assert(!tools.has(options.name));
     tools.set(options.name,factory as Function);
    },
    on(name:string,handler:Function) {assert.equal(name,'before_tool_call');assert(!hook);hook=handler;},
   };
   entry.register(api as unknown as OpenClawPluginApi);
   assert.equal(tools.size,26);assert(hook);
   assert.deepEqual([...tools.keys()].sort(),manifest.contracts.tools.toSorted());
   return {
    tool(name:string,context:Record<string,unknown>):AnyAgentTool {
     const factory=tools.get(name);assert(factory,`Missing registered factory ${name}`);
     const tool=factory(context);assert(tool&&!Array.isArray(tool));assert.equal(tool.name,name);
     return tool;
    },
    before(name:string,params:Record<string,unknown>,requester?:Record<string,unknown>) {
     return hook!({toolName:name,params},{requester});
    },
   };
  }
  const first=registry(),second=registry();
  let current=true;
  const owner={requesterSenderId:syntheticOwner,senderIsOwner:true,assertInvocationCurrent(){assert(current,'INVOCATION_EXPIRED');}};
  const list=first.tool('sheets_list',owner);
  const commitName=kind==='grid'?'sheets_update':kind==='tabs'?'sheets_create_tab':kind==='docs'?'docs_apply':'sheets_append';
  const previewTool=first.tool(kind==='tabs'?'sheets_preview_create_tab':kind==='docs'?'docs_preview_change':'sheets_preview_change',owner);
  const append=first.tool(commitName,owner);
  // Both registration and concrete factory construction succeeded without secrets/runtime.
  runtimeAllowed=true;
  process.env.SECRETARY_CORE_TOKEN=syntheticToken;
  process.env.SECRETARY_APPROVAL_TOKEN='synthetic-separate-operator-token';
  async function execute(tool:AnyAgentTool,input:unknown) {
   const result=await tool.execute('synthetic-entry-call',input);
   assert.deepEqual(JSON.parse(result.content[0].text),result.details,'Host result text/details must agree');
   return result.details as any;
  }
  assert.equal((await execute(list,{})).ok,true,'Lazy bridge must use invocation-time credentials');
  assert.equal((await execute(first.tool('sheets_list',{}),{})).error.code,'OWNER_REQUIRED');
  current=false;
  assert.equal((await execute(list,{})).error.code,'INVOCATION_EXPIRED');
  current=true;
  const change=kind==='grid'?{operation_id:'acceptance-sdk-grid-entry',spreadsheet:'operations',view:'overview',action:'update',fields:{range:'B5:C5',values:[['SDK',false]]}}:kind==='tabs'?{operation_id:'acceptance-sdk-tab-entry',spreadsheet:'operations',title:'SDK plan',rows:10,columns:2,headers:['Task','Owner']}:kind==='docs'?{operation_id:'acceptance-sdk-docs-entry',document:'notes',tab_id:'t.0',action:'append_text',fields:{text:' SDK appended'}}:{operation_id:'acceptance-sdk-entry',spreadsheet:'wedding-demo',view:'tasks',action:'append',fields:{title:'Synthetic SDK entry task',status:'todo'}};
  const preview=await execute(previewTool,change);assert.equal(preview.ok,true);
  const immutable=structuredClone(preview.data);
  if(kind==='grid')preview.data.after[0][0].stringValue='model-altered';else if(kind==='docs')preview.data.after.text='Model-altered text';else preview.data.after.title='Model-altered text';preview.data.preview_digest='model-altered';
  const requester={senderId:syntheticOwner,senderIsOwner:true};
  assert.equal((await first.before(commitName,{operation_id:change.operation_id})).block,true);
  assert.equal((await first.before(commitName,{operation_id:change.operation_id},{senderId:'other',senderIsOwner:true})).block,true);
  assert.equal((await first.before(kind==='grid'?'sheets_append':'sheets_update',{operation_id:change.operation_id},requester)).block,true);
  assert.equal((await second.before(commitName,{operation_id:change.operation_id},requester)).block,true,'Preview must not cross host API instances');
  assert.equal(await first.before('sheets_list',{},requester),undefined,'Read operations must not request approval');
  const decision=await first.before(commitName,{operation_id:change.operation_id},requester);
  assert(!decision.block);const approval=decision.requireApproval;assert(approval);
  assert(approval.description.includes(JSON.stringify(immutable,null,2)));
  assert(!approval.description.includes('model-altered'));
  assert.deepEqual(approval.allowedDecisions,['allow-once','deny']);
  await approval.onResolution('allow-once');
  await assert.rejects(approval.onResolution('allow-once'),/APPROVAL_ALREADY_RESOLVED/);
  assert.equal((await execute(append,{operation_id:change.operation_id})).data.state,'applied');
  assert.equal((await execute(append,{operation_id:change.operation_id})).data.state,'applied');
  if(kind==='docs'){
   const context=await execute(first.tool('docs_read',owner),{document:'notes',mode:'summary_context',max_characters:100});assert.equal(context.data.excerpts[0].excerpt_id,'S1');assert.equal(context.data.coverage.projected_text_complete,true);assert(context.data.excerpts[0].text.includes(' SDK appended'));
   const selectedSummary=await execute(first.tool('docs_read',owner),{documents:['notes','reference'],mode:'summary_context',max_characters:300});assert.equal(selectedSummary.data.documents[0].source_id,'D1');assert.equal(selectedSummary.data.documents[1].context.source.document,'reference');assert.equal(selectedSummary.data.coverage.total_documents,2);
   const readParameters=first.tool('docs_read',owner).parameters as any;assert.equal(readParameters.properties.max_characters.maximum,10000);assert.equal(readParameters.oneOf.length,3);
   const read=await execute(first.tool('docs_read',owner),{document:'notes',tab_id:'t.0'});
   assert.equal(read.data.text.split(' SDK appended').length-1,1);
   const found=await execute(first.tool('docs_search',owner),{document:'notes',query:'SDK appended'});assert.equal(found.data.matches.length,1);assert.equal(found.data.matches[0].tab_id,'t.0');
   const folded=await execute(first.tool('docs_search',owner),{document:'notes',query:'sdk APPENDED',case_sensitive:false});assert.equal(folded.data.matches.length,1);assert.equal(folded.data.match_mode,'literal_case_insensitive');
   const selected=first.tool('docs_search',owner);
   const many=await execute(selected,{documents:['notes','reference'],query:'SDK appended',case_sensitive:false,limit:1});assert.equal(many.data.sources.length,2);assert.equal(many.data.matches[0].document,'notes');assert(many.data.next_cursor);
   const following=await execute(selected,{documents:['notes','reference'],query:'SDK appended',case_sensitive:false,limit:1,cursor:many.data.next_cursor});assert.equal(following.data.matches[0].document,'reference');assert.equal(following.data.next_cursor,null);
   const parameters=selected.parameters as any;assert.equal(parameters.required.includes('document'),false);assert.equal(parameters.properties.documents.maxItems,3);assert.equal(parameters.oneOf.length,2);
  }else if(kind==='grid'){
   const history=await execute(first.tool('operations_get_history',owner),{operation_id:change.operation_id});assert.deepEqual(history.data.operation.after,immutable.after);
   const selected=first.tool('sheets_search',owner),targets=[{spreadsheet:'operations',view:'overview'},{spreadsheet:'archive',view:'overview'}];
   const cross=await execute(selected,{targets,query:'independent',limit:1});assert.equal(cross.data.sources.length,2);assert.equal(cross.data.records[0].spreadsheet,'operations');assert(cross.data.next_cursor);
   const next=await execute(selected,{targets,query:'independent',limit:1,cursor:cross.data.next_cursor});assert.equal(next.data.records[0].spreadsheet,'archive');assert.equal(next.data.next_cursor,null);
   const parameters=selected.parameters as any;assert.equal(parameters.properties.targets.maxItems,3);assert.equal(parameters.oneOf.length,2);
  }else if(kind==='tabs'){
   const history=await execute(first.tool('operations_get_history',owner),{operation_id:change.operation_id});
   assert.equal(history.data.operation.after.registration,'unregistered');
  }else{
  const read=await execute(first.tool('sheets_read',owner),{spreadsheet:'wedding-demo',view:'tasks'});
  assert.equal(read.data.records.filter((row:any)=>row.source.record_id===immutable.record_id).length,1);
  }
  console.log(JSON.stringify({acceptance:'passed',scenario:'sdk-entry',scope:'built plugin + actual SDK registration -> real Core -> simulated Google; host registry fixture only, no live approval UI'}));
 } finally {
  if(saved.core===undefined)delete process.env.SECRETARY_CORE_TOKEN;else process.env.SECRETARY_CORE_TOKEN=saved.core;
  if(saved.approval===undefined)delete process.env.SECRETARY_APPROVAL_TOKEN;else process.env.SECRETARY_APPROVAL_TOKEN=saved.approval;
 }
}
