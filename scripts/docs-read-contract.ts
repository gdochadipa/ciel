/** Validate published Workspace read contracts using the locked SDK dependency graph. */
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';

const contract=JSON.parse(readFileSync(new URL('../contracts/openapi.yaml',import.meta.url),'utf8'));
const ajv=new Ajv2020({strict:false,allErrors:true});
addFormats(ajv);
ajv.addSchema(contract,'urn:ciel:core-contract');
const validate=ajv.compile({$ref:'urn:ciel:core-contract#/paths/~1v1~1tools~1docs_read/post/responses/200/content/application~1json/schema'});
export function assertDocsReadResponse(response:unknown) {
 assert(validate(response),ajv.errorsText(validate.errors));
}

const validateSheets=ajv.compile({$ref:'urn:ciel:core-contract#/paths/~1v1~1tools~1sheets_read/post/responses/200/content/application~1json/schema'});
export function assertSheetsReadResponse(response:unknown) {
 assert(validateSheets(response),ajv.errorsText(validateSheets.errors));
}

const budgetValidators=Object.fromEntries(['sheets_list','sheets_describe','sheets_read','sheets_search','docs_list','docs_describe','docs_read','docs_search'].map(tool=>[tool,ajv.compile({$ref:`urn:ciel:core-contract#/paths/~1v1~1tools~1${tool}/post/responses/200/content/application~1vnd.ciel.read-budget+json/schema`})]));
export function assertReadBudgetResponse(tool:string,response:unknown) {
 const validate=budgetValidators[tool];
 assert(validate,'Unsupported budgeted read tool');
 assert(validate(response),ajv.errorsText(validate.errors));
}
