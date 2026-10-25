/** Disposable actual Core mutation workflow; only downstream Google is simulated. */
import assert from 'node:assert/strict';
import {createSecretary} from '../plugins/secretary-tools/src/bridge';
import {acceptanceConfiguration,syntheticOwner,syntheticToken} from './acceptance';
const approvalToken='synthetic-separate-operator-token';
export async function runWriteAcceptance(coreUrl:string,providerUrl:string) {
 const base=acceptanceConfiguration(coreUrl,'writes');
 const provider=new URL(providerUrl);
 assert.equal(provider.protocol,'http:');assert.equal(provider.hostname,'127.0.0.1');assert.equal(provider.pathname,'/google/v4');assert(!provider.username&&!provider.password&&!provider.search&&!provider.hash);
 const bridge=createSecretary({...base,approvalToken,approvalTransportQualified:true});
 const owner={requesterSenderId:syntheticOwner,senderIsOwner:true};
 async function tool(name:string,input:unknown) {return await bridge.execute(name,input,owner) as any;}
 async function operator(id:string,action:string,input:unknown,token=approvalToken) {
  const response=await fetch(`${coreUrl}/v1/operations/${id}/${action}`,{method:'POST',headers:{authorization:`Bearer ${token}`,'content-type':'application/json'},body:JSON.stringify(input),signal:AbortSignal.timeout(5000)});
  return await response.json() as any;
 }
 async function providerCall(path:string,input:unknown) {
  const response=await fetch(`${providerUrl.replace(/\/v4$/,'')}${path}`,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(input),signal:AbortSignal.timeout(5000)});
  assert(response.ok,`Synthetic provider call failed ${response.status}`);return response.json();
 }
 await providerCall('/__fixture/reset',{});
 const change={operation_id:'acceptance-task',spreadsheet:'wedding-demo',view:'tasks',action:'append',fields:{title:'Synthetic follow-up',status:'todo'}};
 await assert.rejects(bridge.execute('sheets_preview_change',change,{requesterSenderId:'other',senderIsOwner:true}),/OWNER_REQUIRED/);
 const preview=await tool('sheets_preview_change',change);assert.equal(preview.ok,true);assert.equal(preview.data.actor,syntheticOwner);
 assert.equal((await tool('sheets_append',{operation_id:change.operation_id})).ok,false,'Unapproved commit must fail');
 assert.equal((await tool('sheets_preview_change',{...change,fields:{title:'Altered payload',status:'todo'}})).error.code,'operation_conflict');
 await assert.rejects(tool('sheets_append',{operation_id:change.operation_id,approved:true}),/INVALID_INPUT/);
 assert.equal((await operator(change.operation_id,'approval',{decision:'approve',preview_digest:preview.data.preview_digest},syntheticToken)).ok,false,'Business bearer must not authorize operator endpoint');
 assert.equal((await operator(change.operation_id,'approval',{decision:'approve',preview_digest:'wrong-digest'})).ok,false);
 assert.equal((await operator(change.operation_id,'approval',{decision:'approve',preview_digest:preview.data.preview_digest})).data.state,'approved');
 const committed=await tool('sheets_append',{operation_id:change.operation_id});assert.equal(committed.data.state,'applied');
 assert.equal((await tool('sheets_append',{operation_id:change.operation_id})).data.state,'applied');
 let tasks=await tool('sheets_read',{spreadsheet:'wedding-demo',view:'tasks'});assert.equal(tasks.data.records.length,1);
 const recordId=tasks.data.records[0].source.record_id;assert.equal(recordId,preview.data.record_id);
 assert.equal(tasks.data.records[0].fields.title,'Synthetic follow-up');
 const update={operation_id:'acceptance-update',spreadsheet:'wedding-demo',view:'tasks',action:'update',record_id:recordId,fields:{status:'done'}};
 const updated=await tool('sheets_preview_change',update);assert.equal(updated.ok,true);
 await operator(update.operation_id,'approval',{decision:'approve',preview_digest:updated.data.preview_digest});
 assert.equal((await tool('sheets_update',{operation_id:update.operation_id})).data.state,'applied');
 tasks=await tool('sheets_read',{spreadsheet:'wedding-demo',view:'tasks'});assert.equal(tasks.data.records[0].fields.status,'done');assert.equal(tasks.data.records[0].fields.title,'Synthetic follow-up');
 const stale={...update,operation_id:'acceptance-stale',fields:{status:'blocked'}};
 const stalePreview=await tool('sheets_preview_change',stale);assert.equal(stalePreview.ok,true);
 await operator(stale.operation_id,'approval',{decision:'approve',preview_digest:stalePreview.data.preview_digest});
 // External editor uses the real provider API; Core must detect the intervening change.
 await providerCall('/v4/spreadsheets/demo-spreadsheet/values:batchUpdate',{valueInputOption:'RAW',data:[{range:"'AI_Tasks'!C2",majorDimension:'ROWS',values:[['External synthetic edit']]}]});
 assert.equal((await tool('sheets_update',{operation_id:stale.operation_id})).data.state,'conflict');
 tasks=await tool('sheets_read',{spreadsheet:'wedding-demo',view:'tasks'});assert.equal(tasks.data.records[0].fields.title,'External synthetic edit');assert.equal(tasks.data.records[0].fields.status,'done');
 const payment={operation_id:'acceptance-lost',spreadsheet:'wedding-demo',view:'payments',action:'append',fields:{amount:100,currency:'IDR',payment_date:'2027-01-01',cost_group:'synthetic-group',note:'Synthetic response-loss exercise'}};
 const paymentPreview=await tool('sheets_preview_change',payment);assert.equal(paymentPreview.ok,true);
 await operator(payment.operation_id,'approval',{decision:'approve',preview_digest:paymentPreview.data.preview_digest});
 await providerCall('/__fixture/lose-next-response',{});
 assert.equal((await tool('sheets_append',{operation_id:payment.operation_id})).data.state,'uncertain');
 assert.equal((await tool('sheets_append',{operation_id:payment.operation_id})).ok,false,'Uncertain append must never blindly resend');
 assert.equal((await operator(payment.operation_id,'reconcile',{})).data.state,'applied');
 const payments=await tool('sheets_read',{spreadsheet:'wedding-demo',view:'payments'});assert.equal(payments.data.records.length,1);assert.equal(payments.data.records[0].fields.amount,100);assert.equal(payments.data.records[0].source.record_id,paymentPreview.data.record_id);
 const history=await tool('sheets_get_history',{operation_id:payment.operation_id});assert.equal(history.data.operation.state,'applied');assert.equal(history.data.operation.preview_digest,paymentPreview.data.preview_digest);
 console.log(JSON.stringify({acceptance:'passed',scenario:'writes',scope:'actual bridge/Core with stateful Google-only fixture; no live approval transport proof'}));
}
