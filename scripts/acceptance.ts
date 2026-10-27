/** Actual public bridge -> real Core -> simulated Google acceptance. No fetch stubs. */
import assert from 'node:assert/strict';
import {createSecretary} from '../plugins/secretary-tools/src/bridge';
export const syntheticOwner='+15555550123';
export const syntheticToken='synthetic-acceptance-core-token';
export function acceptanceConfiguration(coreUrl:string, scenario:string) {
 const url=new URL(coreUrl);
 if(url.origin!=='http://127.0.0.1:8787'||url.pathname!=='/'||url.search||url.hash||url.username||url.password) throw Error('Acceptance requires disposable Core at http://127.0.0.1:8787');
 if(!['read','error','slow','drift','retry','writes','workspace'].includes(scenario)) throw Error('Unsupported acceptance scenario');
 return {coreUrl:url.origin,coreToken:syntheticToken,owner:syntheticOwner};
}
export function simulationGoogleBase(log:string,route="google",version="v4") {
 const urls:string[]=[];
 function collect(value:unknown) {
  if(typeof value==='string' && /^http:\/\/(?:127\.0\.0\.1|localhost):[0-9]+(?:\/|$)/.test(value)) urls.push(value);
  else if(Array.isArray(value)) value.forEach(collect);
  else if(value && typeof value==='object') Object.values(value).forEach(collect);
 }
 for(const line of log.trim().split('\n')) {const event=JSON.parse(line);if(JSON.stringify(event).includes('"listening"')) collect(event);}
 const reported=urls.find(value=>new URL(value).pathname.replace(/\/$/,'')===`/${route}`)??(route==='google'?urls.find(value=>new URL(value).pathname==='/'):undefined);
 if(!reported) throw Error('Listening event did not report a loopback member/server URL; inspect actual CLI event before wiring');
 const url=new URL(reported);if(url.username||url.password||url.search||url.hash) throw Error('Invalid simulation listening URL');
 url.hostname='127.0.0.1';url.pathname=`/${route}/${version}`;return url.toString().replace(/\/$/,'');
}
export async function runAcceptance(coreUrl:string,scenario:string) {
 if(scenario==='workspace'){
  const {runWorkspaceAcceptance}=await import('./acceptance-workspace');
  return runWorkspaceAcceptance(coreUrl,process.env.SECRETARY_ACCEPTANCE_DOCS_URL??'');
 }
 const bridge=createSecretary(acceptanceConfiguration(coreUrl,scenario));
 const owner={requesterSenderId:syntheticOwner,senderIsOwner:true};
 let ready=false;
 for(let attempt=0;attempt<50;attempt++) {
  try {const result=await bridge.execute('sheets_list',{},owner) as any;if(result.ok===true) {ready=true;break;}}
  catch {} // readiness only, no mutations retried
  await Bun.sleep(200);
 }
 assert(ready,'Real Core did not answer its public list route; no acceptance evidence');
 const list=await bridge.execute('sheets_list',{},owner) as any;
 assert.equal(list.data.spreadsheets[0].spreadsheet,'wedding-demo');
 await assert.rejects(bridge.execute('sheets_read',{spreadsheet:'wedding-demo',view:'wedding_costs'},{}),/OWNER_REQUIRED/);
 const denied=await bridge.execute('sheets_describe',{spreadsheet:'not-approved'},owner) as any;
 assert.equal(denied.ok,false);assert.equal(denied.error.code,'spreadsheet_not_registered');
 const wrongToken=createSecretary({...acceptanceConfiguration(coreUrl,scenario),coreToken:'synthetic-wrong-token'});
 const auth=await wrongToken.execute('sheets_list',{},owner) as any;
 assert.equal(auth.error.code,'unauthorized');
 if(scenario==='slow') {
  const started=Date.now();
  await assert.rejects(bridge.execute('sheets_read',{spreadsheet:'wedding-demo',view:'wedding_costs'},owner),/CORE_UNAVAILABLE_OUTCOME_UNKNOWN/);
  assert(Date.now()-started<7000,'Bridge timeout exceeded bounded allowance');
 } else if(scenario==='error'||scenario==='drift') {
  const failure=await bridge.execute('sheets_read',{spreadsheet:'wedding-demo',view:'wedding_costs'},owner) as any;
  assert.equal(failure.ok,false);assert.equal(failure.error.code,scenario==='error'?'provider_unavailable':'schema_drift');
 } else if(scenario==='writes') {
  const {runWriteAcceptance}=await import('./acceptance-writes');
  await runWriteAcceptance(coreUrl,process.env.SECRETARY_ACCEPTANCE_PROVIDER_URL??'');
 } else {
  const read=await bridge.execute('sheets_read',{spreadsheet:'wedding-demo',view:'wedding_costs',limit:2},owner) as any;
  assert.equal(read.ok,true);assert.equal(read.data.records.length,2);
  assert.equal(read.data.records[0].source.sheet_id,101);
  assert.equal(read.data.records[0].source.spreadsheet_id,'demo-spreadsheet');
  assert(read.data.records[0].source.retrieved_at);
  const page=await bridge.execute('sheets_read',{spreadsheet:'wedding-demo',view:'wedding_costs',limit:2,cursor:read.data.next_cursor},owner) as any;
  assert.equal(page.ok,true);assert.notDeepEqual(page.data.records,read.data.records);
  const unknownView=await bridge.execute('sheets_read',{spreadsheet:'wedding-demo',view:'arbitrary_schema'},owner) as any;
  assert.equal(unknownView.error.code,'invalid_input');
  const search=await bridge.execute('sheets_search',{spreadsheet:'wedding-demo',view:'wedding_costs',query:'Synthetic item 2'},owner) as any;
  assert.equal(search.ok,true);assert(search.data.records.length>0);
  const report=await bridge.execute('reports_wedding',{spreadsheet:'wedding-demo'},owner) as any;
  assert.equal(report.data.known_cost_total,2200);
  assert.equal(report.data.currency_status,'unconfirmed');
  assert.equal(report.data.paid_total,null);assert.equal(report.data.outstanding_balance,null);
  assert.equal(report.data.coverage,'incomplete');assert(report.data.unknown_cost_groups>0);
 }
 console.log(JSON.stringify({acceptance:'passed',scenario,scope:'actual public bridge -> real Core -> simulated Google; no live host or delivery proof'}));
}
if(import.meta.main) {
 if(process.argv[2]==='--simulation-docs-base') console.log(simulationGoogleBase(await Bun.file(process.argv[3]!).text(),'google-docs','v1'));
 else if(process.argv[2]==='--simulation-base') console.log(simulationGoogleBase(await Bun.file(process.argv[3]!).text()));
 else await runAcceptance(process.env.SECRETARY_ACCEPTANCE_CORE_URL??'http://127.0.0.1:8787',process.argv[2]??'read');
}
