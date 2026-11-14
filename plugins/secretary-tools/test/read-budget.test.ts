import {test,expect} from 'bun:test';
import {createSecretary,secretaryModelText} from '../src/bridge';

const identity={requesterSenderId:'owner',senderIsOwner:true};
const id='11111111-1111-4111-8111-111111111111';
function grant(rows=100){return Response.json({grant:{id,rows,document_characters:10000,output_bytes:4096},model_text:'',code:'admitted'});}
function wire(text:string){return Response.json({result:JSON.parse(text),model_text:text},{headers:{'x-ciel-budgeted-result':'v1','content-type':'application/vnd.ciel.read-budget+json'}});}
function bridge(){return createSecretary({owner:'owner',coreToken:'synthetic-core-token',readOnlyRuntime:true});}

test('budgeted output preserves the exact Core numeric/Unicode model string',async()=>{
 const original=globalThis.fetch;const calls:string[]=[];
 const text='{"ok":true,"data":{"value":1e+20,"text":"é🙂"}}';
 globalThis.fetch=(async(url,init)=>{
  const path=new URL(String(url)).pathname;calls.push(path);
  if(path==='/v1/read-budget/reserve')return grant();
  expect(new Headers(init?.headers).get('x-ciel-read-reservation')).toBe(id);
  return wire(text);
 }) as typeof fetch;
 try {
  const result=await bridge().execute('sheets_list',{},identity);
  expect(secretaryModelText(result)).toBe(text);
  expect(JSON.stringify(result)).not.toBe(text);
  expect(calls).toEqual(['/v1/read-budget/reserve','/v1/tools/sheets_list']);
 }finally{globalThis.fetch=original;}
});

test('invalid local input is reserved and settled without a provider tool request',async()=>{
 const original=globalThis.fetch;const calls:string[]=[];
 const text='{"ok":false,"error":{"code":"invalid_input"}}';
 globalThis.fetch=(async(url,init)=>{
  const path=new URL(String(url)).pathname;calls.push(path);
  if(path==='/v1/read-budget/reserve')return grant();
  expect(path).toBe('/v1/read-budget/local-error');
  expect(JSON.parse(String(init?.body))).toEqual({reservation:id,tool:'sheets_list'});
  return wire(text);
 }) as typeof fetch;
 try {
  const result=await bridge().execute('sheets_list',{actor:'forged'},identity);
  expect(secretaryModelText(result)).toBe(text);
  expect(calls).toEqual(['/v1/read-budget/reserve','/v1/read-budget/local-error']);
 }finally{globalThis.fetch=original;}
});

test('denials cannot create unlimited model-visible budget error output',async()=>{
 const original=globalThis.fetch;let calls=0;
 const notice='{"ok":false,"error":{"code":"read_budget_increase_required"}}';
 globalThis.fetch=(async()=>Response.json({grant:null,model_text:calls++===0?notice:'',code:'read_budget_exhausted'})) as typeof fetch;
 try {
  const tool=bridge();
  expect(secretaryModelText(await tool.execute('docs_list',{},identity))).toBe(notice);
  expect(secretaryModelText(await tool.execute('docs_list',{},identity))).toBe('');
  expect(secretaryModelText(await tool.execute('docs_list',{},identity))).toBe('');
  expect(secretaryModelText(await tool.execute('docs_apply',{operation_id:'fake'},identity))).toBe('');
  expect(calls).toBe(3);
 }finally{globalThis.fetch=original;}
});

test('lost post-dispatch response emits one bounded error and never resends',async()=>{
 const original=globalThis.fetch;const calls:string[]=[];
 globalThis.fetch=(async(url)=>{
  const path=new URL(String(url)).pathname;calls.push(path);
  if(path==='/v1/read-budget/reserve')return grant();
  throw Error('synthetic lost response');
 }) as typeof fetch;
 try {
  const result=await bridge().execute('sheets_list',{},identity);
  expect(new TextEncoder().encode(secretaryModelText(result)).length).toBeLessThanOrEqual(256);
  expect(calls).toEqual(['/v1/read-budget/reserve','/v1/tools/sheets_list']);
 }finally{globalThis.fetch=original;}
});

test('remaining rows bound normal and summary pages without changing caller input',async()=>{
 const original=globalThis.fetch;const sent:unknown[]=[];
 globalThis.fetch=(async(url,init)=>{
  if(new URL(String(url)).pathname==='/v1/read-budget/reserve')return grant(2);
  sent.push(JSON.parse(String(init?.body)));return wire('{"ok":true,"data":{}}');
 }) as typeof fetch;
 try {
  const tool=bridge();
  const page=Object.freeze({spreadsheet:'operations',view:'overview',limit:50});
  const summary=Object.freeze({spreadsheet:'operations',view:'overview',mode:'summary_context',max_records:25});
  await tool.execute('sheets_read',page,identity);await tool.execute('sheets_read',summary,identity);
  expect(sent).toEqual([{...page,limit:2},{...summary,max_records:2}]);
  expect(page.limit).toBe(50);expect(summary.max_records).toBe(25);
 }finally{globalThis.fetch=original;}
});
