/** Actual built SDK factory -> protected Core HTTP/bus/journal -> synthetic Google. */
import assert from 'node:assert/strict';
import entry from '../plugins/secretary-tools/dist/index.js';
const [coreUrl,owner]=process.argv.slice(2);
assert.equal(new URL(coreUrl).hostname,'127.0.0.1');
assert.equal(process.env.SECRETARY_APPROVAL_TOKEN,undefined);
function registry(){
 const tools=new Map();let hook;
 const api={pluginConfig:{owner,coreUrl,readOnlyRuntime:true},get runtime(){throw Error('Readonly fixture must not access native runtime capabilities');},registerTool(factory,options){tools.set(options.name,factory);},on(name,handler){assert.equal(name,'before_tool_call');hook=handler;}};
 entry.register(api);
 return {tool(name){const factory=tools.get(name);assert.equal(factory.contextVersion,2);return factory.create({requesterSenderId:owner,senderIsOwner:true,assertInvocationCurrent(){}});},hook};
}
const first=registry();let charged=256;
async function run(registry,name,input){
 const result=await registry.tool(name).execute('repeated-sdk-call-id',input);
 assert.equal(result.content.length,1);
 const text=result.content[0].text;
 assert.deepEqual(JSON.parse(text),result.details);
 charged+=Math.max(256,new TextEncoder().encode(text).length);
 return result;
}
assert.equal((await run(first,'sheets_read',{spreadsheet:'operations',view:'overview',limit:1})).details.ok,true);
assert.equal((await run(first,'docs_read',{document:'notes',tab_id:'t.0',limit:2})).details.data.text,'é🙂');
assert.equal((await run(first,'sheets_list',{actor:'forged'})).details.error.code,'invalid_input');
assert.equal((await run(registry(),'sheets_list',{})).details.ok,true);
const notice=await first.tool('sheets_list').execute('budget-stop',{});
assert.equal(notice.content.length,1);assert.equal(JSON.parse(notice.content[0].text).error.code,'read_budget_increase_required');
assert.equal((await first.tool('sheets_list').execute('repeat-stop',{})).content.length,0);
assert.equal((await first.tool('docs_apply').execute('forbidden-write',{operation_id:'synthetic'})).content.length,0);
assert.equal((await first.tool('reminders_create').execute('forbidden-job',{})).content.length,0);
assert.equal((await first.hook({toolName:'docs_apply',params:{operation_id:'synthetic'}},{})).block,true);
console.log(JSON.stringify({acceptance:'passed',scenario:'protected-read-budget-sdk',charged_bytes:charged}));
