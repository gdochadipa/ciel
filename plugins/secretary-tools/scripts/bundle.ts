// The locked SDK compiler preserves literal Node/SDK imports without a module-location shim.
import {build} from 'esbuild';
import {readFile} from 'node:fs/promises';
const license=await readFile(new URL('../../../LICENSE',import.meta.url),'utf8');
await build({entryPoints:['src/index.ts'],outfile:'dist/index.js',bundle:true,format:'esm',platform:'node',target:'node24',external:['openclaw/*'],banner:{js:`/*!${license}*/`},logLevel:'info'});
