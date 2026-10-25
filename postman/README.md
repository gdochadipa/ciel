# Local secretary dependency acceptance

Only **Google Sheets** is simulated. `scripts/acceptance.sh` launches real Rust Core; `scripts/acceptance.ts` calls real exported bridge handlers over HTTP into Core. Core itself is never a simulation member and fetch is never replaced with a stub.

The local custom mock began with `postman mock generate postman/contracts/google-sheets-read.openapi.yaml`, then gained strict query matching and synthetic grid data. The source is the official [spreadsheets.get reference](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets/get) and [Sheet/GridData reference](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets/sheets), observed 2026-10-25. This is a bounded read subset, not a complete Google contract. Owner-published mock search was skipped; no authenticated owner workspace search, cloud initialization, publication or deployment was performed. Fixtures contain synthetic item labels/amounts and approved schema metadata, never personal workbook contents. Local `require('./fixture.json')` handlers deliberately cannot be deployed unchanged to cloud.

Prerequisites: Postman CLI 1.75+, Bun, Rust dependencies/lockfile available to `cargo run --offline --locked`, free loopback ports 4900 and 8787, no existing Core at 8787. Use only the disposable fixture configuration. Run from the repository:

```sh
bash scripts/acceptance.sh read
bash scripts/acceptance.sh retry
bash scripts/acceptance.sh error
bash scripts/acceptance.sh drift
bash scripts/acceptance.sh slow
bun test scripts/acceptance.test.ts plugins/secretary-tools/test
SECRETARY_HTTP_TESTS=1 bun test plugins/secretary-tools/test
```

The read case checks listing, strict owner and bearer denial, unregistered spreadsheet and unknown view rejection, source evidence, pagination, literal search and wedding report ambiguity. Expected synthetic known cost is 2200; currency, payment balance and completion remain unknown. `retry` serves two 503 responses before succeeding, exercising real Core retries when run. `error` injects persistent 503; `slow` delays beyond the bridge's five second budget; `drift` changes a required header and expects safe rejection. Each simulation has exactly one Google member. All unsupported routes return 501. No mock scheduler, Core, WhatsApp or memory service is introduced. Mutation/stateful write acceptance remains pending T004's final contract.

The native RPC adapter fixture checks execution/completion independently of delivery evidence through the public reminder history tool. It is not live Gateway, restart persistence or WhatsApp proof. The HTTP tests remain actual loopback stand-ins and are opt-in because sandbox bind is prohibited.

Observed 2026-10-25: `bash scripts/acceptance.sh read` failed before listening with `listen EPERM: operation not permitted 127.0.0.1`. No Core or Google dependency request ran. Pure checks pass; this is **acceptance infrastructure with environment-blocked execution**, not an end-to-end pass. Retain simulation NDJSON in private temporary output and inspect member request events after a socket-capable run; injected condition failures are not member request events. No log containing request bodies is committed.
