// Local bounded Sheets get subset, independent of wedding/domain templates.
const http=require('http');
const fields='spreadsheetId,sheets(properties(sheetId,title),merges,data(startRow,startColumn,rowData(values(userEnteredValue,effectiveValue,formattedValue))))';
const book={spreadsheetId:'generic-spreadsheet',sheets:[{properties:{sheetId:7,title:'Operations'},data:[{startRow:1,startColumn:1,rowData:[{values:[{effectiveValue:{stringValue:'Independent'}},{effectiveValue:{numberValue:12}},{effectiveValue:{boolValue:true}}]},{values:[{effectiveValue:{stringValue:'Another'}},{userEnteredValue:{formulaValue:'=2*3'},effectiveValue:{numberValue:6}}]}]}]}]};
http.createServer((req,res)=>{
 const url=new URL(req.url,'http://127.0.0.1');
 const send=(status,body)=>{res.writeHead(status,{'content-type':'application/json'});res.end(JSON.stringify(body));};
 if(req.method!=='GET'||url.pathname!=='/v4/spreadsheets/generic-spreadsheet')return send(501,{error:{code:501,message:'Endpoint not modeled'}});
 if(url.searchParams.get('includeGridData')!=='true'||url.searchParams.get('fields')!==fields||JSON.stringify(url.searchParams.getAll('ranges'))!==JSON.stringify(["'Operations'!B2:D8"]))return send(400,{error:{code:400,message:'Invalid bounded grid request'}});
 return send(200,book);
}).listen(process.env.PORT||4901);
