// Built entry and pinned SDK; fixture registration only, no Core/provider IO.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import entry from '../dist/index.js';
import {acceptanceHooks} from '../../../scripts/acceptance-hooks.mjs';

test('memory and read-only acceptance registries admit lifecycle hooks and retain the approval callback',async()=>{
 for(const pluginConfig of [{owner:'owner',summaryMemoryEnabled:true},{owner:'owner',readOnlyRuntime:true}]){
  const hooks=acceptanceHooks(),tools=new Map();
  const api={pluginConfig,on:hooks.on,registerTool(factory,options){tools.set(options.name,factory);},get runtime(){throw Error('Registration accessed native runtime');}};
  entry.register(api);
  assert.equal(tools.size,33);assert([...tools.values()].every(factory=>factory.contextVersion===2));
  assert.deepEqual(hooks.names().sort(),['before_reset','before_tool_call','session_end']);
  assert.equal(await hooks.beforeToolCall({toolName:'sheets_list',params:{}},{}),undefined);
  if(pluginConfig.readOnlyRuntime===true)assert.deepEqual(await hooks.beforeToolCall({toolName:'docs_apply',params:{}},{}),{block:true,blockReason:'Read-only runtime forbids mutations'});
 }
});

test('fixture registration rejects unexpected and duplicate hooks and never replaces the approval callback',()=>{
 const hooks=acceptanceHooks();
 assert.throws(()=>hooks.beforeToolCall({},{}),/Missing approval fixture hook/);
 assert.throws(()=>hooks.on('unexpected',()=>{}),/Unexpected fixture hook/);
 assert.throws(()=>hooks.on('before_reset',null),/Invalid fixture hook handler/);
 assert.deepEqual(hooks.names(),[]);
 hooks.on('before_tool_call',value=>value);
 assert.throws(()=>hooks.on('before_tool_call',()=>false),/Duplicate fixture hook/);
 assert.equal(hooks.beforeToolCall('original'),'original');
});
