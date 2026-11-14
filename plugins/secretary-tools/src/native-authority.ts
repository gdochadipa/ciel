import type {OpenClawPluginToolContext} from 'openclaw/plugin-sdk/plugin-entry';

type Conversation = {agentId?:string;sessionKey?:string;sessionId?:string};
type Requester = Conversation & {requester?:{senderId?:string;senderIsOwner?:boolean};abortSignal?:AbortSignal};
type Invocation = OpenClawPluginToolContext<2>;
type Binding = {tool:string;context:Invocation};
const MAX_BINDINGS=64;
function fail(code:string):never {throw new Error(code);}

/** Capture host fields once, preserving the host's live owner predicate. */
export function bindNativeInvocation(context:OpenClawPluginToolContext,signal?:AbortSignal):Invocation {
 const assertion=context.assertInvocationCurrent;
 if(typeof assertion!=='function')fail('NATIVE_AUTHORITY_REQUIRED');
 return {
  ...context,
  get senderIsOwner(){return context.senderIsOwner;},
  assertInvocationCurrent(){
   if(signal?.aborted===true)fail('INVOCATION_EXPIRED');
   assertion();
  },
 };
}

function sameConversation(left:Conversation,right:Conversation):boolean {
 return left.agentId===right.agentId && left.sessionKey===right.sessionKey && left.sessionId===right.sessionId;
}
function complete(context:Conversation):boolean {
 return [context.agentId,context.sessionKey,context.sessionId].every(value=>typeof value==='string' && value.length>0 && value.length<=512);
}
function current(context:Invocation):boolean {
 try {context.assertInvocationCurrent();return context.senderIsOwner===true;}
 catch {return false;} // Unusable bindings are discarded, never treated as authority.
}

/** Bounded lookup of live host closures; these entries are neither task grants nor model input. */
export class NativeApprovalAuthority {
 private bindings:Binding[]=[];

 remember(tool:string,context:Invocation):void {
  this.bindings=this.bindings.filter(binding=>current(binding.context));
  if(!complete(context) || !current(context))return;
  if(this.bindings.some(binding=>binding.tool===tool && binding.context===context))return;
  // Refuse new lookup authority at capacity; never evict a live prompt's binding.
  if(this.bindings.length>=MAX_BINDINGS)return;
  this.bindings.push({tool,context});
 }

 capture(tool:string,request:Requester):Invocation {
  if(!complete(request) || request.abortSignal===undefined)fail('NATIVE_AUTHORITY_REQUIRED');
  this.bindings=this.bindings.filter(binding=>current(binding.context));
  const [binding,...remaining]=this.bindings.filter(binding=>binding.tool===tool && sameConversation(binding.context,request));
  if(binding===undefined || remaining.length!==0)fail('NATIVE_AUTHORITY_UNAVAILABLE');
  const context=bindNativeInvocation(binding.context,request.abortSignal);
  const senderId=request.requester?.senderId;
  const senderIsOwner=request.requester?.senderIsOwner;
  const assertion=context.assertInvocationCurrent;
  return {...context,get senderIsOwner(){return context.senderIsOwner;},assertInvocationCurrent(){
   assertion();
   if(context.senderIsOwner!==true || senderIsOwner!==true || senderId!==context.requesterSenderId)fail('OWNER_REQUIRED');
  }};
 }
}
