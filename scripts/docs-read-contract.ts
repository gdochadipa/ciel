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
