import type {OpenClawPluginApi,OpenClawPluginToolContext} from 'openclaw/plugin-sdk/plugin-entry';
import {bindNativeInvocation} from './native-authority.js';

export type NativeSessionIdentity={agentId?:string;sessionKey?:string;sessionId?:string};
const MAX_ENDED_SESSIONS=4096;

/** Local revocation supplements the host assertion; detached hooks cannot replace it. */
export class NativeSessionLifecycle {
 private readonly ended=new Set<string>();
 private saturated=false;

 end(identity:NativeSessionIdentity):void {
  const key=this.key(identity);
  if(key===undefined || this.ended.has(key))return;
  // Never evict a revocation and accidentally revive a retained task callback.
  // Restarting the plugin still requires new host authority and Core reconfirmation.
  if(this.ended.size>=MAX_ENDED_SESSIONS){this.saturated=true;return;}
  this.ended.add(key);
 }

 bind(context:OpenClawPluginToolContext,signal?:AbortSignal):OpenClawPluginToolContext<2> {
  const native=bindNativeInvocation(context,signal),key=this.key(native);
  const check=():void=>{
   native.assertInvocationCurrent();
   if(this.saturated)throw new Error('NATIVE_LIFECYCLE_CAPACITY');
   if(key!==undefined && this.ended.has(key))throw new Error('INVOCATION_EXPIRED');
  };
  return {...native,get senderIsOwner(){return native.senderIsOwner;},assertInvocationCurrent:check};
 }

 private key(identity:NativeSessionIdentity):string|undefined {
  if(![identity.agentId,identity.sessionKey,identity.sessionId].every(value=>typeof value==='string' && value.length>0 && value.length<=512))return undefined;
  return JSON.stringify([identity.agentId,identity.sessionKey,identity.sessionId]);
 }
}

const lifecycles=new WeakMap<OpenClawPluginApi,NativeSessionLifecycle>();
function lifecycleFor(api:OpenClawPluginApi):NativeSessionLifecycle {
 let lifecycle=lifecycles.get(api);
 if(lifecycle===undefined){lifecycle=new NativeSessionLifecycle();lifecycles.set(api,lifecycle);}
 return lifecycle;
}

/** Use this same binding for model factories and trusted native task composition. */
export function bindNativeSessionInvocation(api:OpenClawPluginApi,context:OpenClawPluginToolContext,signal?:AbortSignal):OpenClawPluginToolContext<2> {
 return lifecycleFor(api).bind(context,signal);
}

const registered=new WeakSet<OpenClawPluginApi>();
const endHandlers=new WeakMap<OpenClawPluginApi,(identity:NativeSessionIdentity)=>Promise<void>>();
/** Revoke locally first; an optional trusted task adapter retires only enrolled contexts. */
export function registerNativeSessionLifecycle(api:OpenClawPluginApi,onEnded?:(identity:NativeSessionIdentity)=>Promise<void>):void {
 if(onEnded!==undefined)endHandlers.set(api,onEnded);
 if(registered.has(api))return;
 const lifecycle=lifecycleFor(api);
 function end(context:NativeSessionIdentity):Promise<void>|undefined {
  const captured={agentId:context.agentId,sessionKey:context.sessionKey,sessionId:context.sessionId};
  lifecycle.end(captured);return endHandlers.get(api)?.(captured);
 }
 api.on('before_reset',(_event,context)=>end(context));
 api.on('session_end',(event,context)=>{
  // A contradictory payload is unusable; it must not revoke a successor.
  if(event.sessionId!==context.sessionId || (event.sessionKey!==undefined && context.sessionKey!==undefined && event.sessionKey!==context.sessionKey))return;
  return end(context);
 });
 registered.add(api);
}
