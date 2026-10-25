# Bridge/Core interface conventions

This is the implementation contract seed. `openapi.yaml` is the final machine-readable authority and must agree with implemented public behavior.

- `POST /v1/tools/{name}` accepts the tool's input object directly and returns `{ "ok": true, "data": ... }` or `{ "ok": false, "error": { "code": "...", "message": "...", "retryable": false } }`. Use conventional HTTP status codes as well. Error bodies and logs never include upstream credentials or raw upstream errors.
- All business requests require `Authorization: Bearer <core_token>`. The service configuration binds that token to the sole owner actor. Do not take actor, approval, host, paths or credential IDs from tool inputs.
- Operator-only `POST /v1/operations/{operation_id}/approval` uses a separate `approval_token`, accepts `{decision:"approve"|"reject",preview_digest:string}`, and is never registered as an LLM-callable tool. Native trusted hook callbacks may use this endpoint only when separately qualified and configured. Core writes stay disabled by default.
- `sheets_list`: `{}`.
- `sheets_describe`: `{spreadsheet:string}`.
- `sheets_read`: `{spreadsheet:string,view:string,limit?:number,cursor?:string}`.
- `sheets_search`: `{spreadsheet:string,view:string,query:string,limit?:number,cursor?:string}`. Literal text search only; no query language or arbitrary range.
- `sheets_preview_change`: `{operation_id:string,spreadsheet:string,view:string,action:"append"|"update",record_id?:string,fields:object}`. Field keys must come from the registered writable schema. Preview returns at least operation_id, preview_digest, actor, before/after, expires_at, state.
- `sheets_append` and `sheets_update`: `{operation_id:string}`. They commit only the exact authorized preview and enforce its action. No `approved` input.
- `sheets_get_history`: `{operation_id?:string,limit?:number,cursor?:string}`.
- `reports_wedding`: `{spreadsheet:string}`; reports distinguish numeric known costs from unknowns, currency ambiguity and incomplete coverage. Never derive paid balance from budget or funding-source fields.
- Source responses carry spreadsheet ID, actual sheet ID, source range, record identity if established, retrieval time and mapping version. The XLSX itself is not committed and is never uploaded automatically.
- Reminder tools live in the TypeScript plugin and use verified native `cron.*` gateway RPC. They never implement timing or manipulate native persistence files.
- Unknown fields must be rejected at mutation boundaries. Defaults: 64 KiB request body, 2 MiB provider response, 100 returned records maximum, 5s bridge/core and 10s provider timeout, at most two read retries. Never automatically retry an uncertain write.

Implementation tests should use public tool routes and external HTTP stand-ins. Production dependency availability and live runtime compatibility are separately evidenced.
