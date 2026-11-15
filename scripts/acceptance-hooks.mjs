import assert from 'node:assert/strict';

/** Registration boundary shared by disposable host fixtures, never host authority. */
export function acceptanceHooks() {
 const handlers=new Map();
 return {
  on(name,handler) {
   assert(['before_tool_call','before_reset','session_end'].includes(name),'Unexpected fixture hook');
   assert.equal(typeof handler,'function','Invalid fixture hook handler');assert(!handlers.has(name),'Duplicate fixture hook');
   handlers.set(name,handler);
  },
  names:()=>[...handlers.keys()],
  beforeToolCall(...args) {
   const handler=handlers.get('before_tool_call');assert(handler,'Missing approval fixture hook');
   return handler(...args);
  },
 };
}
