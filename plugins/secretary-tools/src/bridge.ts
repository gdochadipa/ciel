export type Identity = { requesterSenderId?: string; senderIsOwner?: boolean; assertInvocationCurrent?: () => void };
export type Gateway = { isAvailable(): boolean | Promise<boolean>; request<T = unknown>(method: string, params?: Record<string,unknown>): Promise<T> };
type ApprovalDecision = 'allow-once' | 'allow-always' | 'deny' | 'timeout' | 'cancelled';
export type Options = { coreUrl?: string; coreToken: string; owner: string; timeoutMs?: number; maxResponseBytes?: number; approvalToken?: string; approvalTransportQualified?: boolean; gateway?: Gateway; schedulerQualified?: boolean };
export const docsTools=['docs_list','docs_describe','docs_read','docs_search','docs_preview_change','docs_apply','operations_get_history'] as const;
export const coreTools = [...docsTools,'sheets_preview_create_tab','sheets_create_tab','sheets_list','sheets_describe','sheets_read','sheets_search','sheets_preview_change','sheets_append','sheets_update','sheets_get_history','reports_wedding'] as const;
export const reminderTools = ['create','list','get','reschedule','pause','resume','cancel','history'].map(action=>`reminders_${action}`);
export const secretaryToolNames = [...coreTools,...reminderTools];
const fields: Record<string, [string[], string[]]> = {
 docs_list:[[],[]],docs_describe:[['document'],[]],docs_read:[[],['document','documents','tab_id','limit','cursor','mode','max_characters']],docs_search:[['query'],['document','documents','tab_id','limit','cursor','case_sensitive']],docs_preview_change:[['operation_id','document','tab_id','action','fields'],[]],docs_apply:[['operation_id'],[]],operations_get_history:[[],['operation_id','limit','cursor']],
 sheets_preview_create_tab:[['operation_id','spreadsheet','title','rows','columns'],['headers']],sheets_create_tab:[['operation_id'],[]],
 sheets_list:[[],[]], sheets_describe:[['spreadsheet'],[]], sheets_read:[['spreadsheet','view'],['limit','cursor']], sheets_search:[['query'],['spreadsheet','view','targets','limit','cursor']],
 sheets_preview_change:[['operation_id','spreadsheet','view','action','fields'],['record_id']], sheets_append:[['operation_id'],[]], sheets_update:[['operation_id'],[]], sheets_get_history:[[],['operation_id','limit','cursor']], reports_wedding:[['spreadsheet'],[]],
};
export class SecretaryError extends Error { constructor(public code: string) { super(code); } }
function fail(code: string): never { throw new SecretaryError(code); }
function object(value: unknown): Record<string, unknown> { if (!value || typeof value !== 'object' || Array.isArray(value)) fail('INVALID_INPUT'); return value as Record<string,unknown>; }
function shape(value: unknown, required: string[], optional: string[] = []) { const input = object(value); if (required.some(k => !(k in input)) || Object.keys(input).some(k => ![...required,...optional].includes(k))) fail('INVALID_INPUT'); return input; }
function isReminderJob(job: Record<string,unknown>): boolean {
 const payload=job.payload;
 return job.sessionTarget==='main' && job.delivery===undefined && !!payload && typeof payload==='object' && !Array.isArray(payload) && (payload as Record<string,unknown>).kind==='systemEvent';
}
export const DEFAULT_CORE_URL = 'http://127.0.0.1:8787';
const canceledReminderDescription = 'Secretary: canceled; retained for history';
export function createSecretary(options: Options) {
 const previews = new Map<string,{preview:Record<string,unknown>;expiresAt:number;bytes:number}>();
 let previewBytes=0;
 const forgetPreview=(id:string)=>{
  const previous=previews.get(id);
  if(previous) {previewBytes-=previous.bytes;previews.delete(id);}
 };
 const prunePreviews=()=>{
  const now=Date.now();
  for(const [id,entry] of previews)if(entry.expiresAt<=now)forgetPreview(id);
 };
 const rememberPreview=(id:string,preview:Record<string,unknown>)=>{
  prunePreviews();forgetPreview(id);
  const expiresAt=Date.parse(String(preview.expires_at));
  if(!Number.isFinite(expiresAt)||expiresAt<=Date.now())return;
  const encoded=JSON.stringify(preview),bytes=new TextEncoder().encode(encoded).length;
  // Bound transient approval state without a timer or a second persistent store.
  if(bytes>4194304)return;
  while(previews.size>=64||previewBytes+bytes>4194304)forgetPreview(previews.keys().next().value!);
  previews.set(id,{preview:JSON.parse(encoded),expiresAt,bytes});previewBytes+=bytes;
 };
 const url = new URL(options.coreUrl ?? DEFAULT_CORE_URL);
 if (url.protocol !== 'http:' || !['127.0.0.1','[::1]'].includes(url.hostname) || url.username || url.password || url.search || url.hash || url.pathname !== '/') fail('INVALID_CORE_DESTINATION');
 if (!options.coreToken || !options.owner) fail('INVALID_CONFIGURATION');
 const authorize = (identity: Identity) => { if (identity.senderIsOwner !== true || identity.requesterSenderId !== options.owner) fail('OWNER_REQUIRED'); identity.assertInvocationCurrent?.(); };
 async function request(path: string, input: unknown, identity:Identity, approval = false) {
  authorize(identity);
  const body = JSON.stringify(input); if (new TextEncoder().encode(body).length > 65536) fail('REQUEST_TOO_LARGE');
  let response: Response;
  try { response = await fetch(new URL(path, url), {method:'POST',redirect:'error',headers:{'content-type':'application/json',authorization:`Bearer ${approval ? options.approvalToken : options.coreToken}`},body,signal:AbortSignal.timeout(options.timeoutMs ?? 5000)}); }
  catch { fail('CORE_UNAVAILABLE_OUTCOME_UNKNOWN'); }
  const reader = response.body?.getReader(); const chunks: Uint8Array[] = []; let size = 0;
  try { if(reader) while(true) { const part = await reader.read(); if(part.done) break; size += part.value.length; if(size > (options.maxResponseBytes ?? 2097152)) { await reader.cancel(); fail('RESPONSE_TOO_LARGE'); } chunks.push(part.value); } }
  catch (error) { if(error instanceof SecretaryError) throw error; fail('CORE_UNAVAILABLE_OUTCOME_UNKNOWN'); }
  authorize(identity);
  const buffer = new Uint8Array(size); let offset=0; for(const chunk of chunks) {buffer.set(chunk,offset);offset+=chunk.length;}
  let result: Record<string,unknown>; try {result=object(JSON.parse(new TextDecoder().decode(buffer)));} catch {fail('INVALID_CORE_RESPONSE');}
  if (result.ok !== true) { const error = object(result.error); return {ok:false,error:{code:typeof error.code==='string' && /^[A-Za-z_0-9]{1,64}$/.test(error.code) ? error.code : 'CORE_ERROR',message:'Core rejected the request',retryable:false}}; }
  if (!response.ok) fail('INVALID_CORE_RESPONSE'); return result;
 }
 async function execute(name: string, value: unknown, identity: Identity) {
  authorize(identity);
  if (name.startsWith('reminders_')) return reminder(name, value, identity);
  if (!fields[name]) fail('UNKNOWN_TOOL');
  const input = shape(value,...fields[name]);
  if(name==='docs_search' && (('document' in input)===('documents' in input) || ('documents' in input && 'tab_id' in input))) fail('INVALID_INPUT');
  if(name==='sheets_search' && ('targets' in input ? ('spreadsheet' in input || 'view' in input) : !('spreadsheet' in input && 'view' in input))) fail('INVALID_INPUT');
  if(name==='docs_read' && ((('document' in input)===('documents' in input)) || ('mode' in input ? (input.mode!=='summary_context' || ['tab_id','limit','cursor'].some(key=>key in input)) : (!('tab_id' in input) || 'max_characters' in input || 'documents' in input)))) fail('INVALID_INPUT');
  for (const [key,val] of Object.entries(input)) {
   if(key==='max_characters') {if(!Number.isInteger(val) || Number(val)<('documents' in input?300:100) || Number(val)>10000) fail('INVALID_INPUT');continue;}
   if(key==='targets') {
    if(!Array.isArray(val) || val.length<1 || val.length>3) fail('INVALID_INPUT');
    const pairs=val.map(target=>{
     const pair=shape(target,['spreadsheet','view']);
     if(Object.values(pair).some(alias=>typeof alias!=='string' || !alias.trim() || new TextEncoder().encode(alias).length>100)) fail('INVALID_INPUT');
     return JSON.stringify([pair.spreadsheet,pair.view]);
    });
    if(new Set(pairs).size!==pairs.length) fail('INVALID_INPUT');continue;
   }
   if(key==='documents') {
    if(!Array.isArray(val) || val.length<1 || val.length>3 || new Set(val).size!==val.length || val.some(alias=>typeof alias!=='string' || !alias.trim() || new TextEncoder().encode(alias).length>100)) fail('INVALID_INPUT');
    continue;
   }
   if (key==='rows' || key==='columns') {if(!Number.isInteger(val) || Number(val)<1 || Number(val)>(key==='rows'?1000:26)) fail('INVALID_INPUT');continue;}
   if(key==='headers') {if(!Array.isArray(val) || val.length>Number(input.columns) || val.some(h=>typeof h!=='string' || new TextEncoder().encode(h).length>256 || /[\x00-\x1f\x7f-\x9f]/.test(h))) fail('INVALID_INPUT');continue;}
   if(key==='case_sensitive') {if(typeof val!=='boolean') fail('INVALID_INPUT');continue;}
   if (key==='fields') {object(val);continue;}
   if (key==='limit') {if(!Number.isInteger(val) || Number(val)<1 || Number(val)>(name==='docs_read'?10000:name==='docs_search'?50:100)) fail('INVALID_INPUT');continue;}
   if(typeof val !=='string' || !val.trim() || val.length>8192) fail('INVALID_INPUT');
  }
  if((name==='docs_search' || (name==='sheets_search' && 'targets' in input)) && (!String(input.query).trim() || new TextEncoder().encode(String(input.query)).length>256)) fail('INVALID_INPUT');
  if(name==='sheets_preview_create_tab' && (Number(input.rows)*Number(input.columns)>10000 || String(input.title).trim()!==input.title || [...String(input.title)].length>100 || /[\x00-\x1f\x7f-\x9f\[\]:*?/\\]/.test(String(input.title)))) fail('INVALID_INPUT');
  if((name==='sheets_preview_change' && !['append','update'].includes(String(input.action))) || (name==='docs_preview_change' && !['append_text','replace_text'].includes(String(input.action)))) fail('INVALID_INPUT');
  if(['sheets_append','sheets_update','docs_apply','sheets_create_tab'].includes(name) && (!options.approvalTransportQualified || !options.approvalToken)) fail('WRITES_DISABLED');
  const result=await request(`/v1/tools/${name}`,input,identity);
  if(['sheets_preview_change','docs_preview_change','sheets_preview_create_tab'].includes(name) && result.ok===true) {
   const preview=object(result.data);
   if(preview.operation_id!==input.operation_id || typeof preview.preview_digest!=='string' || typeof preview.actor!=='string' || typeof preview.expires_at!=='string') fail('INVALID_CORE_RESPONSE');
   if(name==='docs_preview_change' && (preview.resource_kind!=='google_docs' || preview.document!==input.document || preview.tab_id!==input.tab_id || preview.action!==input.action || preview.actor!==options.owner)) fail('INVALID_CORE_RESPONSE');
   if(name==='sheets_preview_create_tab') {
    const expected={title:input.title,rows:input.rows,columns:input.columns,headers:input.headers??[]};
    if(preview.resource_kind!=='sheets_tab' || preview.spreadsheet!==input.spreadsheet || preview.action!=='create_tab' || preview.actor!==options.owner || JSON.stringify(Object.entries(object(preview.fields)).sort())!==JSON.stringify(Object.entries(expected).sort())) fail('INVALID_CORE_RESPONSE');
   }
   if(name==='sheets_preview_change' && (![undefined,'sheets','sheets_grid'].includes(preview.resource_kind as string|undefined) || preview.actor!==options.owner || preview.spreadsheet!==input.spreadsheet || preview.view!==input.view || preview.action!==input.action || JSON.stringify(Object.entries(object(preview.fields)).sort())!==JSON.stringify(Object.entries(object(input.fields)).sort()) || (preview.resource_kind==='sheets_grid' && input.action!=='update'))) fail('INVALID_CORE_RESPONSE');
   rememberPreview(String(input.operation_id),preview);
  }
  return result;
 }
 async function reminder(name: string, value: unknown, identity: Identity): Promise<unknown> {
  const gateway=options.gateway;
  if(!options.schedulerQualified || !gateway || await gateway.isAvailable()!==true) fail('SCHEDULER_UNAVAILABLE');
  const rpc=async(method:string,params:Record<string,unknown>)=>{
   authorize(identity);
   const result=await gateway.request(method,params);
   authorize(identity);
   return result;
  };
  const action=name.slice('reminders_'.length);
  const allowed: Record<string,[string[],string[]]>={create:[['reminder_id','text','schedule'],[]],list:[[],[]],get:[['reminder_id'],[]],reschedule:[['reminder_id','schedule'],[]],pause:[['reminder_id'],[]],resume:[['reminder_id'],[]],cancel:[['reminder_id'],[]],history:[['reminder_id'],[]]};
  if(!allowed[action]) fail('UNKNOWN_TOOL');
  const input=shape(value,...allowed[action]);
  if(input.reminder_id!==undefined && (typeof input.reminder_id!=='string' || !/^[a-zA-Z0-9_-]{1,80}$/.test(input.reminder_id))) fail('INVALID_INPUT');
  const prefix=`secretary:${options.owner}:`;
  const key=prefix+input.reminder_id;
  const schedule=input.schedule===undefined?undefined:exactSchedule(input.schedule);
  if(action==='create' && (typeof input.text!=='string' || !input.text.trim() || input.text.length>2000)) fail('INVALID_INPUT');
  // No arbitrary delivery target: main-session events wake the owner agent only.
  const allJobs:Record<string,unknown>[]=[];
  const seenIds=new Set<string>();
  let offset=0, responseBytes=0;
  for(let page=0;;page++) {
   if(page>=10) fail('SCHEDULER_LIST_TOO_LARGE');
   authorize(identity);
   const result=object(await rpc('cron.list',{includeDisabled:true,limit:200,offset,sortBy:'name',sortDir:'asc'}));
   responseBytes+=new TextEncoder().encode(JSON.stringify(result)).length;
   if(responseBytes>(options.maxResponseBytes??2097152)) fail('SCHEDULER_LIST_TOO_LARGE');
   if(!Array.isArray(result.jobs) || result.jobs.length>200 || (result.hasMore!==undefined && typeof result.hasMore!=='boolean')) fail('INVALID_GATEWAY_RESPONSE');
   for(const value of result.jobs) {
    const job=object(value);
    if(typeof job.id!=='string' || !job.id) fail('INVALID_GATEWAY_RESPONSE');
    if(seenIds.has(job.id)) fail('SCHEDULER_SNAPSHOT_CHANGED');
    seenIds.add(job.id);allJobs.push(job);
   }
   if(result.hasMore===false || (result.hasMore===undefined && result.jobs.length<200)) break;
   const nextOffset=result.nextOffset??(result.hasMore===undefined?offset+200:undefined);
   if(!Number.isSafeInteger(nextOffset) || Number(nextOffset)<=offset || result.jobs.length===0) fail('INVALID_GATEWAY_RESPONSE');
   offset=Number(nextOffset);
  }
  const jobs=allJobs.filter(job=>typeof job.declarationKey==='string' && job.declarationKey.startsWith(prefix));
  if(action==='list') return {jobs};
  const matches=jobs.filter((job:any)=>job.declarationKey===key);
  if(matches.length>1) fail('REMINDER_RECONCILIATION_REQUIRED');
  const existing=matches[0] as Record<string,unknown>|undefined;
  if(action==='create') {
   if(existing) {
    if(!isReminderJob(existing)) fail('REMINDER_CONFLICT');
    if(JSON.stringify(existing.schedule)!==JSON.stringify(schedule) || object(existing.payload).text!==input.text) fail('REMINDER_CONFLICT');
    return {reconciled:true,job:existing};
   }
   authorize(identity);
   const added=object(await rpc('cron.add',{name:`Secretary ${input.reminder_id}`,declarationKey:key,schedule,sessionTarget:'main',wakeMode:'now',payload:{kind:'systemEvent',text:input.text},deleteAfterRun:false}));
   return {reconciled:false,job:added.job??added,delivery:'unconfirmed'};
  }
  if(!existing || typeof existing.id!=='string') fail('REMINDER_NOT_FOUND');
  if(action==='get') return {job:await rpc('cron.get',{id:existing.id})};
  if(action==='history') return rpc('cron.runs',{id:existing.id,limit:100});
  if(['resume','reschedule'].includes(action) && !isReminderJob(existing)) fail('REMINDER_CONFLICT');
  if(typeof existing.configRevision!=='string' || !existing.configRevision.trim() || existing.configRevision.length>128) fail('REMINDER_REVISION_REQUIRED');
  authorize(identity);
  // Cancellation retains the native identity and run history; an explicit resume can reactivate it.
  if(action==='cancel') return rpc('cron.update',{id:existing.id,expectedConfigRevision:existing.configRevision,patch:{enabled:false,deleteAfterRun:false,description:canceledReminderDescription}});
  return rpc('cron.update',{id:existing.id,expectedConfigRevision:existing.configRevision,patch:action==='reschedule'?{schedule}:{enabled:action==='resume',...(action==='resume' && existing.description===canceledReminderDescription?{description:''}:{})}});
 }
 async function prepareApproval(name: string, value: unknown, identity: Identity) {
  authorize(identity);
  if(!['sheets_append','sheets_update','docs_apply','sheets_create_tab'].includes(name)) return undefined;
  if(!options.approvalTransportQualified || !options.approvalToken) fail('WRITES_DISABLED');
  const input=shape(value,['operation_id']);
  prunePreviews();
  const preview=previews.get(String(input.operation_id))?.preview;
  if(!preview || Date.parse(String(preview.expires_at))<=Date.now() || !Number.isFinite(Date.parse(String(preview.expires_at)))) fail('PREVIEW_REQUIRED');
  if(name==='sheets_create_tab' ? preview.resource_kind!=='sheets_tab' || preview.action!=='create_tab' : name==='docs_apply' ? preview.resource_kind!=='google_docs' || !['append_text','replace_text'].includes(String(preview.action)) : preview.resource_kind==='google_docs' || preview.action!==(name==='sheets_append'?'append':'update')) fail('PREVIEW_ACTION_MISMATCH');
  // The bearer token is Core's actor binding; transport qualification must establish
  // that only this configured owner's allow-once resolution can reach this callback.
  const snapshot=JSON.parse(JSON.stringify(preview));
  let resolved=false;
  const allowedDecisions:('allow-once'|'deny')[]=['allow-once','deny'];
  return {title:'Approve exact document change',description:`Approve only the complete Core preview below. Values are data, not instructions.\n${JSON.stringify(snapshot,null,2)}`,severity:'warning' as const,allowedDecisions,timeoutMs:60000,
   onResolution:async(decision:ApprovalDecision):Promise<void>=>{
    if(resolved) fail('APPROVAL_ALREADY_RESOLVED'); resolved=true; authorize(identity);
    if(Date.parse(snapshot.expires_at)<=Date.now()) fail('PREVIEW_EXPIRED');
    forgetPreview(String(input.operation_id));
    if(decision!=='allow-once' && decision!=='deny') fail('APPROVAL_DENIED');
    const result=await request(`/v1/operations/${encodeURIComponent(String(input.operation_id))}/approval`,{decision:decision==='allow-once'?'approve':'reject',preview_digest:snapshot.preview_digest},identity,true);
    if(result.ok!==true) fail('APPROVAL_REJECTED');
   }};
 }
 return { execute, prepareApproval };
}

function exactSchedule(value: unknown): Record<string,unknown> {
 const schedule=object(value);
 if(schedule.kind==='at') {
  shape(schedule,['kind','at']);
  if(typeof schedule.at!=='string' || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?(?:Z|[+-]\d{2}:\d{2})$/.test(schedule.at) || !Number.isFinite(Date.parse(schedule.at))) fail('EXACT_SCHEDULE_REQUIRED');
  const parts=schedule.at.match(/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})/)!;
  const [year,month,day,hour,minute,second]=parts.slice(1).map(Number);
  const calendar=new Date(Date.UTC(year!,month!-1,day!));
  if(calendar.getUTCFullYear()!==year || calendar.getUTCMonth()!==month!-1 || calendar.getUTCDate()!==day || hour!>23 || minute!>59 || second!>59) fail('EXACT_SCHEDULE_REQUIRED');
  return {kind:'at',at:new Date(schedule.at).toISOString()};
 }
 if(schedule.kind==='cron') {
  shape(schedule,['kind','expr'],['tz']);
  if(typeof schedule.expr!=='string' || schedule.expr.trim().split(/\s+/).length!==5 || !/^[0-9*,/\- ]+$/.test(schedule.expr)) fail('EXACT_SCHEDULE_REQUIRED');
  const limits=[59,23,31,12,7], minimum=[0,0,1,1,0];
  for(const [index,part] of schedule.expr.trim().split(/\s+/).entries()) for(const term of part.split(',')) {
   if(!/^(?:\*|\d+(?:-\d+)?)(?:\/[1-9]\d*)?$/.test(term)) fail('EXACT_SCHEDULE_REQUIRED');
   const [base,step]=term.split('/'); const nums=base==='*'?[]:base.split('-').map(Number);
   if(nums.some(n=>n<minimum[index]!||n>limits[index]!) || (nums.length===2 && nums[0]!>nums[1]!) || (step && Number(step)>limits[index]!+1)) fail('EXACT_SCHEDULE_REQUIRED');
  }
  const tz=schedule.tz??'Asia/Makassar'; if(typeof tz!=='string') fail('EXACT_SCHEDULE_REQUIRED');
  try {new Intl.DateTimeFormat('en',{timeZone:tz});} catch {fail('EXACT_SCHEDULE_REQUIRED');}
  return {kind:'cron',expr:schedule.expr.trim().replace(/\s+/g,' '),tz};
 }
 fail('EXACT_SCHEDULE_REQUIRED');
}
