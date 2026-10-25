// LOCAL ONLY. Derived from official Sheets v4 spreadsheets.get contract, not a Core mock.
// Synthetic fixture files are intentionally local; do not deploy this handler to cloud.
const http = require('http');
const fixture = require('./fixture.json');
const fields = 'spreadsheetId,sheets(properties(sheetId,title),merges,data(startRow,startColumn,rowData(values(userEnteredValue,effectiveValue,formattedValue))))';
http.createServer((req,res) => {
 const url = new URL(req.url,'http://127.0.0.1');
 const send=(status,body)=>{res.writeHead(status,{'content-type':'application/json'});res.end(JSON.stringify(body));};
 const match=/^\/v4\/spreadsheets\/([a-zA-Z0-9_-]+)$/.exec(url.pathname);
 if(req.method!=='GET'||!match) return send(501,{error:{code:501,message:'Endpoint not modeled'}});
 if(match[1]!=='demo-spreadsheet') return send(404,{error:{code:404,message:'Requested entity was not found'}});
 // Enforce this caller's documented request, unlike a generated replay accepting any shape.
 if(url.searchParams.get('includeGridData')!=='true'||url.searchParams.get('fields')!==fields||url.searchParams.getAll('ranges').length!==5) return send(400,{error:{code:400,message:'Invalid synthetic acceptance query'}});
 const expected=["'Sheet1'!A1:G63","'Sheet2'!A1:F13","'Sheet3'!A3:C26","'Sheet4'!A1:C10","'RUMAH'!A1:D16"];
 if(expected.some(range=>!url.searchParams.getAll('ranges').includes(range))) return send(400,{error:{code:400,message:'Invalid bounded ranges'}});
 return send(200,fixture);
}).listen(process.env.PORT||4901);
