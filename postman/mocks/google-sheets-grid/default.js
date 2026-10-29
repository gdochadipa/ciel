// Official Sheets get/addSheet/updateCells subset; synthetic data only.
const http=require('http');const reference=require('./reference.json');
const fields='spreadsheetId,sheets(properties(sheetId,title),merges,data(startRow,startColumn,rowData(values(userEnteredValue,effectiveValue,formattedValue))))';
const original={spreadsheetId:'generic-spreadsheet',sheets:[{properties:{sheetId:7,title:'Operations',sheetType:'GRID',gridProperties:{rowCount:100,columnCount:10}},data:[{startRow:1,startColumn:1,rowData:[{values:[{effectiveValue:{stringValue:'Independent'}},{effectiveValue:{numberValue:12}},{effectiveValue:{boolValue:true}}]},{values:[{effectiveValue:{stringValue:'Another'}},{userEnteredValue:{formulaValue:'=2*3'},effectiveValue:{numberValue:6}}]}]}]}]};
const STATE='google-sheets-grid:create-state';
function readJson(req){
 if(req.body!==undefined)return Promise.resolve(req.body);
 return new Promise((resolve,reject)=>{let raw='';req.on('data',c=>{raw+=c;});req.on('end',()=>{try{resolve(raw?JSON.parse(raw):{});}catch{reject(Error('invalid JSON'));}});});
}
http.createServer(async(req,res)=>{
 const pending=readJson(req);
 const url=new URL(req.url,'http://127.0.0.1');
 const send=(status,body)=>{res.writeHead(status,{'content-type':'application/json'});res.end(JSON.stringify(body));};
 try {
 const state=JSON.parse(JSON.stringify((await pm.state.get(STATE))||{book:original,creates:0,lose:false}));
 const book=state.book;
 if(url.pathname==='/__fixture/stats')return send(200,{creates:state.creates,gridWrites:state.gridWrites||0});
 if(req.method==='POST'&&url.pathname==='/__fixture/lose-next-response'){await pending;state.lose=true;await pm.state.set(STATE,state);return send(200,{ok:true});}
 if(req.method==='POST'&&url.pathname==='/v4/spreadsheets/generic-spreadsheet:batchUpdate'){
  const body=await pending;
  const u=body.requests?.[0]?.updateCells;
  if(u && body.requests.length===1 && u.start?.sheetId===7){
   if(u.fields!=='userEnteredValue'||u.start.rowIndex<3||u.start.rowIndex>7||u.start.columnIndex<1||u.start.columnIndex>3||!Array.isArray(u.rows)||u.start.rowIndex+u.rows.length>8)return send(400,{error:{code:400}});
   const rows=book.sheets[0].data[0].rowData;
   for(let r=0;r<u.rows.length;r++){
    const values=u.rows[r].values;if(!Array.isArray(values)||u.start.columnIndex+values.length>4)return send(400,{error:{code:400}});
    const index=u.start.rowIndex+r-1;while(rows.length<=index)rows.push({values:[]});
    for(let c=0;c<values.length;c++){const cell=values[c];if(cell.userEnteredValue?.formulaValue!==undefined)return send(400,{error:{code:400}});const col=u.start.columnIndex+c-1;while(rows[index].values.length<=col)rows[index].values.push({});rows[index].values[col]=cell;}
   }
   state.gridWrites=(state.gridWrites||0)+1;const lose=state.lose;state.lose=false;await pm.state.set(STATE,state);
   if(lose){res.writeHead(200,{'content-type':'application/json'});return res.end('{incomplete');}return send(200,{replies:[{}]});
  }
  const requests=body.requests,p=requests?.[0]?.addSheet?.properties;
  if(!p||!Number.isInteger(p.sheetId)||p.sheetId<0||p.sheetType!=='GRID'||p.gridProperties?.rowCount<1||p.gridProperties.rowCount>1000||p.gridProperties.columnCount<1||p.gridProperties.columnCount>26||p.gridProperties.rowCount*p.gridProperties.columnCount>10000||book.sheets.some(s=>s.properties.sheetId===p.sheetId||s.properties.title.toLowerCase()===p.title.toLowerCase())||requests.length>2)return send(400,{error:{code:400}});
  const sheet={properties:p};
  if(requests.length===2){const u=requests[1].updateCells;if(!u||u.start.sheetId!==p.sheetId||u.start.rowIndex!==0||u.start.columnIndex!==0||u.fields!=='userEnteredValue'||u.rows.length!==1||u.rows[0].values.length>p.gridProperties.columnCount||u.rows[0].values.some(c=>typeof c.userEnteredValue?.stringValue!=='string'||Object.keys(c.userEnteredValue).length!==1))return send(400,{error:{code:400}});sheet.data=[{rowData:u.rows}];}
  book.sheets.push(sheet);state.creates++;const lose=state.lose;state.lose=false;await pm.state.set(STATE,state);
  if(lose){res.writeHead(200,{'content-type':'application/json'});return res.end('{incomplete');}
  return send(200,{replies:requests.map((_,i)=>i===0?{addSheet:{properties:p}}:{})});
 }
 if(req.method==='GET'&&url.pathname==='/v4/spreadsheets/reference-spreadsheet') {
  if(url.searchParams.get('includeGridData')!=='true'||url.searchParams.get('fields')!==fields||url.searchParams.getAll('ranges').length!==1||url.searchParams.get('ranges')!=="'Archive'!B2:D8")return send(400,{error:{code:400}});
  return send(200,reference);
 }
 if(req.method!=='GET'||url.pathname!=='/v4/spreadsheets/generic-spreadsheet')return send(501,{error:{code:501,message:'Endpoint not modeled'}});
 if(url.searchParams.get('includeGridData')==='false'&&url.searchParams.get('fields')==='spreadsheetId,sheets(properties(sheetId,title,sheetType,gridProperties(rowCount,columnCount)))')return send(200,{spreadsheetId:book.spreadsheetId,sheets:book.sheets.map(s=>({properties:s.properties}))});
 const ranges=url.searchParams.getAll('ranges');
 if(url.searchParams.get('includeGridData')!=='true'||ranges.length!==1)return send(400,{error:{code:400}});
 if(url.searchParams.get('fields')===fields&&ranges[0]==="'Operations'!B2:D8")return send(200,{spreadsheetId:book.spreadsheetId,sheets:[book.sheets[0]]});
 const match=/^'((?:[^']|'')+)'!A1:([A-Z])1$/.exec(ranges[0]);
 const sheet=match&&book.sheets.find(s=>s.properties.title===match[1].replace(/''/g,"'"));
 if(!sheet||url.searchParams.get('fields')!=='spreadsheetId,sheets(properties(sheetId,title),data(startRow,startColumn,rowData(values(userEnteredValue))))')return send(400,{error:{code:400}});
 return send(200,{spreadsheetId:book.spreadsheetId,sheets:[sheet]});
 }catch{return send(400,{error:{code:400,message:'Invalid synthetic request'}});}
}).listen(process.env.PORT||4901);
