// Local Google Docs text/revision subset. Never a Core or host approval mock.
const http=require('http');const seed=require('./fixture.json');const reference=require('./reference.json');
const STATE='google-docs-text:state';
function readJson(req){
 if(req.body!==undefined)return Promise.resolve(req.body);
 return new Promise((resolve,reject)=>{let raw='';req.on('data',chunk=>{raw+=chunk;if(raw.length>65536)reject(Error('too large'));});req.on('end',()=>{try{resolve(raw?JSON.parse(raw):{});}catch{reject(Error('invalid JSON'));}});});
}
function text(tab){return tab.documentTab.body.content.flatMap(v=>v.paragraph?.elements??[]).map(v=>v.textRun?.content??'').join('');}
function setText(tab,value){tab.documentTab.body.content=[{paragraph:{elements:[{textRun:{content:value}}]}}];}
function keys(value,expected){return value && typeof value==='object' && !Array.isArray(value) && Object.keys(value).sort().join(',')===expected.slice().sort().join(',');}
http.createServer(async(req,res)=>{
 const pending=readJson(req);
 const send=(status,value)=>{res.writeHead(status,{'content-type':'application/json'});res.end(JSON.stringify(value));};
 try{
  const url=new URL(req.url,'http://127.0.0.1');
  let state=(await pm.state.get(STATE))||{document:seed,revision:1,applies:0,loss:false};state=JSON.parse(JSON.stringify(state));
  if(req.method==='POST'&&url.pathname==='/__fixture/reset'){await pending;await pm.state.set(STATE,{document:seed,revision:1,applies:0,loss:false});return send(200,{reset:true});}
  if(req.method==='POST'&&url.pathname==='/__fixture/lose-next-response'){await pending;state.loss=true;await pm.state.set(STATE,state);return send(200,{armed:true});}
  if(req.method==='POST'&&url.pathname==='/__fixture/external-edit'){
   const body=await pending;if(typeof body.text!=='string')return send(400,{error:{code:400}});
   setText(state.document.tabs[0],body.text);state.document.revisionId=`rev-${++state.revision}`;await pm.state.set(STATE,state);return send(200,{edited:true});
  }
  if(req.method==='GET'&&url.pathname==='/__fixture/stats')return send(200,{applies:state.applies});
  if(req.method==='GET'&&['/v1/documents/demo-document','/v1/documents/reference-document'].includes(url.pathname)){
   if(url.searchParams.get('includeTabsContent')!=='true'||url.searchParams.get('suggestionsViewMode')!=='SUGGESTIONS_INLINE')return send(400,{error:{code:400,message:'Explicit tabs/suggestion view required'}});
   return send(200,url.pathname.endsWith('/reference-document')?reference:state.document);
  }
  if(req.method==='POST'&&url.pathname==='/v1/documents/demo-document:batchUpdate'){
   const body=await pending;
   if(!keys(body,['requests','writeControl'])||!keys(body.writeControl,['requiredRevisionId'])||body.writeControl.requiredRevisionId!==state.document.revisionId||!Array.isArray(body.requests)||body.requests.length!==1)return send(400,{error:{code:400,message:'Invalid request or stale required revision'}});
   const request=body.requests[0];let reply={};
   if(keys(request,['insertText'])){
    const insert=request.insertText;
    if(!keys(insert,['text','endOfSegmentLocation'])||!keys(insert.endOfSegmentLocation,['tabId'])||typeof insert.text!=='string')return send(400,{error:{code:400}});
    const tab=state.document.tabs.find(t=>t.tabProperties.tabId===insert.endOfSegmentLocation.tabId);if(!tab)return send(400,{error:{code:400}});
    setText(tab,text(tab).slice(0,-1)+insert.text+'\n');
   }else if(keys(request,['replaceAllText'])){
    const replace=request.replaceAllText;
    if(!keys(replace,['replaceText','tabsCriteria','containsText'])||!keys(replace.tabsCriteria,['tabIds'])||replace.tabsCriteria.tabIds?.length!==1||!keys(replace.containsText,['text','matchCase','searchByRegex'])||replace.containsText.matchCase!==true||replace.containsText.searchByRegex!==false||typeof replace.containsText.text!=='string'||!replace.containsText.text||typeof replace.replaceText!=='string')return send(400,{error:{code:400}});
    const tab=state.document.tabs.find(t=>t.tabProperties.tabId===replace.tabsCriteria.tabIds[0]);if(!tab)return send(400,{error:{code:400}});
    const before=text(tab),matches=before.split(replace.containsText.text).length-1;
    setText(tab,before.split(replace.containsText.text).join(replace.replaceText));reply={replaceAllText:{occurrencesChanged:matches}};
   }else return send(501,{error:{code:501,message:'Request not modeled'}});
   state.document.revisionId=`rev-${++state.revision}`;state.applies++;const lose=state.loss;state.loss=false;
   await pm.state.set(STATE,state);
   if(lose)return req.socket.destroy();
   return send(200,{documentId:seed.documentId,writeControl:{requiredRevisionId:state.document.revisionId},replies:[reply]});
  }
  return send(501,{error:{code:501,message:'Endpoint not modeled'}});
 }catch{return send(400,{error:{code:400,message:'Invalid synthetic request'}});}
}).listen(process.env.PORT||4901);
