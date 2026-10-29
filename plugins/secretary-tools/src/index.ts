// Host imports deliberately isolated: bridge tests require no substituted SDK.
import type {OpenClawPluginApi,AnyAgentTool} from 'openclaw/plugin-sdk/plugin-entry';
import {defineToolPlugin} from 'openclaw/plugin-sdk/tool-plugin';
import {Type} from 'typebox';
import {createSecretary, reminderTools, secretaryToolNames, type Options} from './bridge.js';
const reminderDescriptions:Record<string,string>={
 reminders_create:'Create a reminder using a stable reminder_id and an exact timestamp with offset or a five-field cron schedule. Clarify ambiguous dates first. Reuse the ID to reconcile a retry; a changed request conflicts. Execution does not prove message delivery.',
 reminders_list:'List owner-scoped native reminders, including disabled and retained completed jobs. A scheduler discovery error means the list is incomplete; do not infer that a reminder is missing.',
 reminders_get:'Read the current native job for a secretary reminder_id.',
 reminders_reschedule:'Change only the schedule of an owner reminder using its current native revision. After an error, read its current state before deciding to retry.',
 reminders_pause:'Disable an owner reminder while retaining its native job and history. After an error, inspect current state before retrying.',
 reminders_resume:'Explicitly re-enable an owner reminder. A changed native payload or outbound route requires reconciliation and cannot be enabled by this tool.',
 reminders_cancel:'Cancel an owner reminder by disabling and retaining the native job for history. Creation retries do not resume canceled jobs.',
 reminders_history:'Read up to 100 recent native executions. Preserve separate execution and delivery statuses; unknown or accepted delivery is not proof of receipt.',
};
const documentDescriptions:Record<string,string>={
 sheets_search:'Search literal values using spreadsheet+view or targets: an explicit ordered list of 1–3 distinct registered {spreadsheet,view} pairs. Never mix selectors. Global limit default50/max100; <=256 UTF-8 query bytes, selected alias/view names <=100 UTF-8 bytes. Existing Sheets lowercase matching, not full Unicode folding. Per-record and all-view source evidence, source-bound global pagination, 128KiB multi page cap and four-second aggregate Core deadline. Whole-page errors; no discovery or writes.',
 sheets_preview_change:'Prepare immutable registered Sheets edit. Structured fields follow its schema. Grid update requires explicit writable_range, action update, no record_id, fields {range,values}: complete rectangle <=100 literal cells; null clears. No formulas/merges/protected headers or structural changes.',
 sheets_update:'Apply only the exact owner-approved registered structured/grid update. Changed source conflicts; ambiguous outcomes require operator observation-only reconciliation, never resending.',

 sheets_preview_create_tab:'Preview a new bounded tab inside a registered spreadsheet with create_tabs permission. Specify title, rows, columns and optional literal text headers. Result remains unregistered and unreadable/unwritable until the operator registers bounded access.',
 sheets_create_tab:'Create only the immutable owner-approved tab preview. Replay does not create another tab. An uncertain outcome requires observation-only operator reconciliation; never retry with a new operation ID.',
 docs_list:'List only operator-registered Google Docs aliases; this does not enumerate Google Drive.',
 docs_describe:'Describe a registered Google Doc, its tab IDs, revision and supported text structure before choosing a tab.',
 docs_search:'Search literal text using document for one registered Google Doc or documents for an explicit list of 1–3 distinct registered readable aliases; never both. documents searches all body tabs, forbids tab_id, returns global pages with per-document evidence, fails as a whole and has a four-second Core deadline. Ordered selection and every source bind its cursor; case_sensitive defaults true, false matches Unicode case variants without normalization. Search all body tabs or optional tab_id. Query <=256 UTF-8 bytes; limit default 10, max 50. Results include bounded snippets and Unicode scalar offsets in projected text, not Google edit indices. Source-bound cursor rejects changed queries/tabs/content. No regex or Drive discovery.',
 docs_read:'Read a bounded body-text excerpt from a registered Google Doc tab. Default 2000 characters; cursor is bound to the source revision. Layout and non-text objects are not reproduced.',
 docs_preview_change:'Prepare an immutable Google Docs text edit. append_text fields: {text}; replace_text fields: {find,replacement}, exactly one case-sensitive match. Select a tab ID from docs_describe. Unsupported structures and suggestions fail closed for editing.',
 docs_apply:'Apply only a previously previewed and owner-approved Google Docs operation_id. Revision changes conflict. After uncertain outcome, inspect operation history and reconcile; never resend blindly.',
 operations_get_history:'Read the owner operation journal for registered Sheets and Google Docs work; this does not authorize retrying a mutation.',
 reports_wedding:'Optional wedding-profile report. Generic grids and Google Docs do not require or infer wedding fields.',
};
const string={type:'string',minLength:1,maxLength:8192};
const specs:Record<string,[string[],Record<string,unknown>]>={
 sheets_preview_create_tab:[['operation_id','spreadsheet','title','rows','columns'],{operation_id:string,spreadsheet:string,title:{type:'string',minLength:1,maxLength:100},rows:{type:'integer',minimum:1,maximum:1000},columns:{type:'integer',minimum:1,maximum:26},headers:{type:'array',maxItems:26,items:{type:'string',maxLength:256}}}],sheets_create_tab:[['operation_id'],{operation_id:string}],
 docs_list:[[],{}],docs_describe:[['document'],{document:string}],docs_read:[['document','tab_id'],{document:string,tab_id:string,limit:{type:'integer',minimum:1,maximum:10000},cursor:string}],docs_search:[['query'],{document:string,documents:{type:'array',minItems:1,maxItems:3,uniqueItems:true,items:{type:'string',minLength:1,maxLength:100}},query:{type:'string',minLength:1,maxLength:256},tab_id:{type:'string',minLength:1,maxLength:200},case_sensitive:{type:'boolean',default:true},limit:{type:'integer',minimum:1,maximum:50},cursor:string}],docs_preview_change:[['operation_id','document','tab_id','action','fields'],{operation_id:string,document:string,tab_id:string,action:{enum:['append_text','replace_text']},fields:{oneOf:[{type:'object',required:['text'],properties:{text:{type:'string',minLength:1,maxLength:8192}},additionalProperties:false},{type:'object',required:['find','replacement'],properties:{find:{type:'string',minLength:1,maxLength:2048},replacement:{type:'string',maxLength:8192}},additionalProperties:false}]}}],docs_apply:[['operation_id'],{operation_id:string}],operations_get_history:[[],{operation_id:string,limit:{type:'integer',minimum:1,maximum:100},cursor:string}],
 sheets_list:[[],{}],sheets_describe:[['spreadsheet'],{spreadsheet:string}],sheets_read:[['spreadsheet','view'],{spreadsheet:string,view:string,limit:{type:'integer',minimum:1,maximum:100},cursor:string}],sheets_search:[['query'],{spreadsheet:string,view:string,targets:{type:'array',minItems:1,maxItems:3,uniqueItems:true,items:{type:'object',required:['spreadsheet','view'],properties:{spreadsheet:{type:'string',minLength:1,maxLength:100},view:{type:'string',minLength:1,maxLength:100}},additionalProperties:false}},query:string,limit:{type:'integer',minimum:1,maximum:100},cursor:string}],
 sheets_preview_change:[['operation_id','spreadsheet','view','action','fields'],{operation_id:string,spreadsheet:string,view:string,action:{enum:['append','update']},record_id:string,fields:{type:'object'}}],sheets_append:[['operation_id'],{operation_id:string}],sheets_update:[['operation_id'],{operation_id:string}],sheets_get_history:[[],{operation_id:string,limit:{type:'integer',minimum:1,maximum:100},cursor:string}],reports_wedding:[['spreadsheet'],{spreadsheet:string}]
};
for(const name of reminderTools) {
 const action=name.slice(10); const required=action==='list'?[]:action==='create'?['reminder_id','text','schedule']:action==='reschedule'?['reminder_id','schedule']:['reminder_id'];
 specs[name]=[required,Object.fromEntries(required.map(key=>[key,key==='schedule'?{oneOf:[{type:'object',required:['kind','at'],properties:{kind:{const:'at'},at:string},additionalProperties:false},{type:'object',required:['kind','expr'],properties:{kind:{const:'cron'},expr:string,tz:string},additionalProperties:false}]}:string]))];
}
const selectors:Record<string,Record<string,unknown>>={
 docs_search:{oneOf:[{required:['document'],not:{required:['documents']}},{required:['documents'],not:{anyOf:[{required:['document']},{required:['tab_id']}]}}]},
 sheets_search:{oneOf:[{required:['spreadsheet','view'],not:{required:['targets']}},{required:['targets'],not:{anyOf:[{required:['spreadsheet']},{required:['view']}]}}]},
};
const schemas=Object.fromEntries(Object.entries(specs).map(([name,[required,properties]])=>[name,{type:'object',additionalProperties:false,required,properties,...(selectors[name]??{})}]));

// Authoring metadata is constructed without reading credentials or connecting to a host.
const bridges=new WeakMap<OpenClawPluginApi,ReturnType<typeof createSecretary>>();
function bridgeFor(api:OpenClawPluginApi) {
 let bridge=bridges.get(api);
 if(!bridge) {
  const config=api.pluginConfig??{};
  const options:Options={coreUrl:typeof config.coreUrl==='string'?config.coreUrl:undefined,owner:typeof config.owner==='string'?config.owner:'',coreToken:process.env.SECRETARY_CORE_TOKEN??'',approvalToken:process.env.SECRETARY_APPROVAL_TOKEN,approvalTransportQualified:config.approvalTransportQualified===true,schedulerQualified:config.schedulerQualified===true,gateway:api.runtime.gateway};
  bridge=createSecretary(options);bridges.set(api,bridge);
 }
 return bridge;
}
const entry=defineToolPlugin({
 id:'secretary-tools',name:'Personal Secretary tools',description:'Bounded owner-only Workspace and native scheduler bridge',
 configSchema:Type.Object({
  owner:Type.String({minLength:1}),
  coreUrl:Type.Optional(Type.String({default:'http://127.0.0.1:8787'})),
  approvalTransportQualified:Type.Optional(Type.Boolean({default:false})),
  schedulerQualified:Type.Optional(Type.Boolean({default:false})),
 },{additionalProperties:false}),
 tools:tool=>secretaryToolNames.map(name=>tool({
  name,label:name,description:documentDescriptions[name]??reminderDescriptions[name]??'Owner-only secretary operation; input cannot set actor, credentials, host or outbound recipient.',
  parameters:Type.Unsafe(schemas[name]),
  factory:({api,toolContext}):AnyAgentTool=>({
   name,label:name,description:documentDescriptions[name]??reminderDescriptions[name]??'Owner-only secretary operation; input cannot set actor, credentials, host or outbound recipient.',
   parameters:Type.Unsafe(schemas[name]),
   async execute(_toolCallId,params) {
    try {
     const value=await bridgeFor(api).execute(name,params,toolContext);
     return {content:[{type:'text',text:JSON.stringify(value)}],details:value};
    } catch(error) {const code=error instanceof Error && /^[A-Z_]+$/.test(error.message)?error.message:'SECRETARY_UNAVAILABLE';return {content:[{type:'text',text:JSON.stringify({ok:false,error:{code}})}],details:{ok:false,error:{code}}};}
   }
  }),
 })),
});
// Preserve the SDK entry and its authoring metadata while adding the native approval hook.
const registerTools=entry.register!;
entry.register=(api)=>{
 registerTools(api);
 api.on('before_tool_call',async(event,context)=>{
  if(!['sheets_append','sheets_update','docs_apply','sheets_create_tab'].includes(event.toolName))return;
  const requester=context.requester;
  try {
   const requireApproval=await bridgeFor(api).prepareApproval(event.toolName,event.params,{requesterSenderId:requester?.senderId,senderIsOwner:requester?.senderIsOwner});
   return {requireApproval};
  } catch {return {block:true,blockReason:'Owner approval transport and an unexpired immutable preview are required'};}
 });
};
export default entry;
