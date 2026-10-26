// Repository metadata checks only. This never imports or activates the OpenClaw SDK.
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { secretaryToolNames } from '../src/bridge';

const fail = (code: string): never => { throw new Error(code); };
function sameStrings(value: unknown, expected: readonly string[]): boolean {
 return Array.isArray(value) && value.length===expected.length && value.every(item=>typeof item==='string') && new Set(value).size===value.length && new Set(expected).size===expected.length && expected.every(item=>value.includes(item));
}
async function metadata(root: string, name: string) {
 try { return JSON.parse(await readFile(resolve(root,name),'utf8')); }
 catch { fail('PACKAGE_METADATA_UNREADABLE'); }
}
try {
 const args=process.argv.slice(2);
 if(args.length!==0 && (args.length!==2 || args[0]!=='--root' || !args[1])) fail('INVALID_PACKAGE_ARGUMENTS');
 const root=args[1]?resolve(args[1]):resolve(dirname(fileURLToPath(import.meta.url)),'..');
 const manifest=await metadata(root,'openclaw.plugin.json');
 const pkg=await metadata(root,'package.json');
 if(!sameStrings(manifest?.contracts?.tools,secretaryToolNames)) fail('TOOL_CONTRACT_MISMATCH');
 if(manifest.id!=='secretary-tools' || manifest.version!==pkg.version || pkg.type!=='module') fail('PACKAGE_IDENTITY_MISMATCH');
 if(manifest.activation?.onStartup!==true) fail('EXPLICIT_STARTUP_ACTIVATION_REQUIRED');
 const schema=manifest.configSchema;
 if(!schema?.required?.includes('owner') || schema.properties?.owner?.type!=='string' || schema.properties.owner.minLength!==1 || schema.additionalProperties!==false) fail('OWNER_CONFIG_REQUIRED');
 if(schema.properties?.approvalTransportQualified?.default!==false || schema.properties?.schedulerQualified?.default!==false) fail('CAPABILITIES_MUST_DEFAULT_OFF');
 if(pkg.peerDependencies?.openclaw!=='2026.10.7' || pkg.engines?.node!=='>=24.16.0') fail('HOST_VERSION_CONTRACT_MISMATCH');
 if(!sameStrings(pkg.openclaw?.extensions,['./dist/index.js'])) fail('BUILT_RUNTIME_ENTRY_REQUIRED');
 if(!sameStrings(pkg.files,['dist/index.js','openclaw.plugin.json','README.md'])) fail('PACKAGE_FILE_ALLOWLIST_REQUIRED');
 console.log('Package metadata only: checks passed; SDK compatibility and activation are unverified.');
} catch(error) {
 console.error(error instanceof Error && /^[A-Z_]+$/.test(error.message)?error.message:'PACKAGE_METADATA_INVALID');
 process.exitCode=1;
}
