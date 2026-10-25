// LOCAL ONLY: official Google values append/batchUpdate subset + explicit fixture controls.
const http=require('http');
const seed=require('./fixture.json');
const BOOK='google-sheets-writes:book', LOSS='google-sheets-writes:lose-next-response';
const fields='spreadsheetId,sheets(properties(sheetId,title),merges,data(startRow,startColumn,rowData(values(userEnteredValue,effectiveValue,formattedValue))))';
const expected=["'Sheet1'!A1:G63","'Sheet2'!A1:F13","'Sheet3'!A3:C26","'Sheet4'!A1:C10","'RUMAH'!A1:D16","'AI_Tasks'!A1:Z1001","'AI_Payments'!A1:Z1001"];
function readJson(req) {
 if(req.body!==undefined) return Promise.resolve(req.body);
 return new Promise((resolve,reject)=>{let raw='';req.on('data',chunk=>{raw+=chunk;if(raw.length>65536) reject(Error('too large'));});req.on('end',()=>{try{resolve(raw?JSON.parse(raw):{});}catch{reject(Error('invalid JSON'));}});});
}
function cell(value) {return {userEnteredValue:{[typeof value==='number'?'numberValue':'stringValue']:value??''},effectiveValue:{[typeof value==='number'?'numberValue':'stringValue']:value??''}};}
http.createServer(async(req,res)=>{
 const bodyPromise=readJson(req); // Attach stream listeners before awaiting Postman state.
 const send=(status,data)=>{res.writeHead(status,{'content-type':'application/json'});res.end(JSON.stringify(data));};
 try {
  const url=new URL(req.url,'http://127.0.0.1');
  if(req.method==='POST' && url.pathname==='/__fixture/reset') {
   await bodyPromise;await pm.state.set(BOOK,seed);await pm.state.set(LOSS,false);return send(200,{reset:true});
  }
  if(req.method==='POST' && url.pathname==='/__fixture/lose-next-response') {
   await bodyPromise;await pm.state.set(LOSS,true);return send(200,{armed:true});
  }
  const book=JSON.parse(JSON.stringify((await pm.state.get(BOOK))||seed));
  if(req.method==='GET' && url.pathname==='/v4/spreadsheets/demo-spreadsheet') {
   const ranges=url.searchParams.getAll('ranges');
   if(url.searchParams.get('fields')!==fields||url.searchParams.get('includeGridData')!=='true'||ranges.length!==expected.length||expected.some(v=>!ranges.includes(v))) return send(400,{error:{code:400,message:'Invalid bounded query'}});
   return send(200,book);
  }
  const append=/^\/v4\/spreadsheets\/demo-spreadsheet\/values\/(.+)$/.exec(url.pathname);
  let result;
  if(req.method==='POST' && append) {
   const range=decodeURIComponent(append[1]);const match=/^'(AI_Tasks|AI_Payments)'!A2:([EG])1001:append$/.exec(range);
   if(!match||url.searchParams.get('valueInputOption')!=='RAW'||url.searchParams.get('insertDataOption')!=='INSERT_ROWS'||url.searchParams.get('includeValuesInResponse')!=='true') return send(400,{error:{code:400,message:'Invalid append'}});
   const body=await bodyPromise;const width=match[1]==='AI_Tasks'?5:7;
   if(body.majorDimension!=='ROWS'||!Array.isArray(body.values)||body.values.length!==1||body.values[0].length!==width||match[2]!==String.fromCharCode(64+width)) return send(400,{error:{code:400,message:'Invalid ValueRange'}});
   const sheet=book.sheets.find(s=>s.properties.title===match[1]);const rows=sheet.data[0].rowData;
   if(rows.length>=1001) return send(400,{error:{code:400,message:'Synthetic bounded table full'}});
   const row=rows.length+1;rows.push({values:body.values[0].map(cell)});
   result={spreadsheetId:book.spreadsheetId,tableRange:row===2?'':`'${match[1]}'!A2:${match[2]}${row-1}`,updates:{spreadsheetId:book.spreadsheetId,updatedRange:`'${match[1]}'!A${row}:${match[2]}${row}`,updatedRows:1,updatedColumns:width,updatedCells:width,updatedData:{range:`'${match[1]}'!A${row}:${match[2]}${row}`,majorDimension:'ROWS',values:body.values}}};
  } else if(req.method==='POST' && url.pathname==='/v4/spreadsheets/demo-spreadsheet/values:batchUpdate') {
   const body=await bodyPromise;
   if(body.valueInputOption!=='RAW'||!Array.isArray(body.data)||!body.data.length||body.data.length>26) return send(400,{error:{code:400,message:'Invalid batch'}});
   const responses=[];const touched=new Set();
   for(const entry of body.data) {
    const match=/^'(AI_Tasks|AI_Payments)'!([B-G])([0-9]+)$/.exec(entry.range??'');
    if(!match||entry.majorDimension!=='ROWS'||entry.values?.length!==1||entry.values[0]?.length!==1) return send(400,{error:{code:400,message:'Invalid bounded cell update'}});
    const sheet=book.sheets.find(s=>s.properties.title===match[1]);const row=Number(match[3]),col=match[2].charCodeAt(0)-65;
    if(row<2||row>1001||!sheet.data[0].rowData[row-1]||col>(match[1]==='AI_Tasks'?4:6)) return send(400,{error:{code:400,message:'Cell outside owned table'}});
    sheet.data[0].rowData[row-1].values[col]=cell(entry.values[0][0]);touched.add(match[1]);
    responses.push({spreadsheetId:book.spreadsheetId,updatedRange:entry.range,updatedRows:1,updatedColumns:1,updatedCells:1});
   }
   result={spreadsheetId:book.spreadsheetId,totalUpdatedCells:responses.length,totalUpdatedSheets:touched.size,responses};
  } else {
   if(/^\/v4\/spreadsheets\/[^/]+(?:$|\/)/.test(url.pathname) && !url.pathname.includes('/demo-spreadsheet')) return send(404,{error:{code:404,message:'Requested entity was not found'}});
   return send(501,{error:{code:501,message:'Endpoint not modeled'}});
  }
  await pm.state.set(BOOK,book); // Persist before intentionally losing the response.
  if(await pm.state.get(LOSS)) {await pm.state.set(LOSS,false);return req.socket.destroy();}
  return send(200,result);
 } catch {return send(400,{error:{code:400,message:'Invalid synthetic request'}});}
}).listen(process.env.PORT||4901);
