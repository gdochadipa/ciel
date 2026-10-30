/** Validate the published docs_read response contract using the locked SDK dependency graph. */
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';

const contract=JSON.parse(readFileSync(new URL('../contracts/openapi.yaml',import.meta.url),'utf8'));
const ajv=new Ajv2020({strict:false,allErrors:true});
addFormats(ajv);
const validate=ajv.compile(contract.paths['/v1/tools/docs_read'].post.responses['200'].content['application/json'].schema);
export function assertDocsReadResponse(response:unknown) {
 assert(validate(response),ajv.errorsText(validate.errors));
}
