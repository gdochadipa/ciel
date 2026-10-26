// Host imports deliberately isolated: bridge tests require no substituted SDK.
import { definePluginEntry, type OpenClawPluginApi, type OpenClawPluginToolContext, type AnyAgentTool } from 'openclaw/plugin-sdk/plugin-entry';
import {createSecretary, coreTools, type Options} from './bridge.js';
const reminderTools=['create','list','get','reschedule','pause','resume','cancel','history'].map(action=>`reminders_${action}`);
export default definePluginEntry({
 id:'secretary-tools', name:'Personal Secretary tools', description:'Bounded owner-only Core and native scheduler bridge',
 register(api: OpenClawPluginApi) {
  // Credentials are operator environment configuration, never tool arguments.
  const config=api.pluginConfig as {coreUrl?:string;owner?:string;approvalTransportQualified?:boolean;schedulerQualified?:boolean};
  const options:Options={coreUrl:config.coreUrl,owner:config.owner??'',coreToken:process.env.SECRETARY_CORE_TOKEN??'',approvalToken:process.env.SECRETARY_APPROVAL_TOKEN,approvalTransportQualified:config.approvalTransportQualified===true,schedulerQualified:config.schedulerQualified===true,gateway:api.runtime.gateway};
  const bridge=createSecretary(options);
  const names=[...coreTools,...reminderTools,'memory_capabilities'];
  api.registerTool((context: OpenClawPluginToolContext): AnyAgentTool[] => names.map(name=>({
   name,label:name,description:name==='memory_capabilities'?'Native memory mutations are unavailable.':'Owner-only secretary operation; input cannot set actor, credentials, host or outbound recipient.',
   parameters: schemas[name]??{type:'object',additionalProperties:false,properties:{}},
   async execute(_toolCallId,params) {
    try {
     const value=await bridge.execute(name,params,context);
     return {content:[{type:'text',text:JSON.stringify(value)}],details:value};
    } catch(error) {const code=error instanceof Error && /^[A-Z_]+$/.test(error.message)?error.message:'SECRETARY_UNAVAILABLE';return {content:[{type:'text',text:JSON.stringify({ok:false,error:{code}})}],details:{ok:false,error:{code}}};}
   }
  })),{names});
  api.on('before_tool_call',async(event,context)=>{
   if(!['sheets_append','sheets_update'].includes(event.toolName)) return;
   const requester=context.requester;
   try {
    const requireApproval=await bridge.prepareApproval(event.toolName,event.params,{requesterSenderId:requester?.senderId,senderIsOwner:requester?.senderIsOwner});
    return {requireApproval};
   } catch {return {block:true,blockReason:'Owner approval transport and an unexpired immutable preview are required'};}
  });
 }
});
const string={type:'string',minLength:1,maxLength:8192};
const specs:Record<string,[string[],Record<string,unknown>]>={
 sheets_list:[[],{}],sheets_describe:[['spreadsheet'],{spreadsheet:string}],sheets_read:[['spreadsheet','view'],{spreadsheet:string,view:string,limit:{type:'integer',minimum:1,maximum:100},cursor:string}],sheets_search:[['spreadsheet','view','query'],{spreadsheet:string,view:string,query:string,limit:{type:'integer',minimum:1,maximum:100},cursor:string}],
 sheets_preview_change:[['operation_id','spreadsheet','view','action','fields'],{operation_id:string,spreadsheet:string,view:string,action:{enum:['append','update']},record_id:string,fields:{type:'object'}}],sheets_append:[['operation_id'],{operation_id:string}],sheets_update:[['operation_id'],{operation_id:string}],sheets_get_history:[[],{operation_id:string,limit:{type:'integer',minimum:1,maximum:100},cursor:string}],reports_wedding:[['spreadsheet'],{spreadsheet:string}]
};
for(const name of reminderTools) {
 const action=name.slice(10); const required=action==='list'?[]:action==='create'?['reminder_id','text','schedule']:action==='reschedule'?['reminder_id','schedule']:['reminder_id'];
 specs[name]=[required,Object.fromEntries(required.map(key=>[key,key==='schedule'?{oneOf:[{type:'object',required:['kind','at'],properties:{kind:{const:'at'},at:string},additionalProperties:false},{type:'object',required:['kind','expr'],properties:{kind:{const:'cron'},expr:string,tz:string},additionalProperties:false}]}:string]))];
}
const schemas=Object.fromEntries(Object.entries(specs).map(([name,[required,properties]])=>[name,{type:'object',additionalProperties:false,required,properties}]));
