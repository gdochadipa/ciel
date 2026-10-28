# Bridge/Core interface conventions

This is the implementation contract seed. `openapi.yaml` is the final machine-readable authority and must agree with implemented public behavior.

- `POST /v1/tools/{name}` accepts the tool's input object directly and returns `{ "ok": true, "data": ... }` or `{ "ok": false, "error": { "code": "...", "message": "...", "retryable": false } }`. Use conventional HTTP status codes as well. Error bodies and logs never include upstream credentials or raw upstream errors.
- All business requests require `Authorization: Bearer <core_token>`. The service configuration binds that token to the sole owner actor. Do not take actor, approval, host, paths or credential IDs from tool inputs.
- Operator-only `POST /v1/operations/{operation_id}/approval` uses a separate `approval_token`, accepts `{decision:"approve"|"reject",preview_digest:string}`, and is never registered as an LLM-callable tool. Native trusted hook callbacks may use this endpoint only when separately qualified and configured. Core writes stay disabled by default.
- `docs_list`: `{}`; returns only operator-registered readable Docs.
- `docs_describe`: `{document:string}`; returns title, revision and tab IDs/structure flags.
- `docs_read`: `{document:string,tab_id:string,limit?:number,cursor?:string}`; body-text excerpt, default 2000/max 10000 Unicode characters. Source-fingerprint-bound cursor fails if the document changes.
- `docs_preview_change`: `{operation_id:string,document:string,tab_id:string,action:"append_text"|"replace_text",fields:object}`. Fields are `{text}` for append or `{find,replacement}` for one exact case-sensitive match. The resource must be registered writable and global writes enabled. Unsupported structures/suggestions/headers fail closed for edit.
- `docs_apply`: `{operation_id:string}`; same qualified hook/owner/one-shot approval as Sheets, with Google requiredRevisionId and observation-based reconciliation after ambiguous outcomes.
- `operations_get_history`: `{operation_id?:string,limit?:number,cursor?:string}`; shared Sheets/Docs operation journal. `sheets_get_history` remains a compatibility route.
- `sheets_list`: `{}`.
- `sheets_describe`: `{spreadsheet:string}`.
- `sheets_read`: `{spreadsheet:string,view:string,limit?:number,cursor?:string}`.
- `sheets_search`: `{spreadsheet:string,view:string,query:string,limit?:number,cursor?:string}`. Literal text search only; no query language or arbitrary range.
- `sheets_preview_change`: `{operation_id:string,spreadsheet:string,view:string,action:"append"|"update",record_id?:string,fields:object}`. Field keys must come from the registered writable schema. Preview returns at least operation_id, preview_digest, actor, before/after, expires_at, state.
- `sheets_append` and `sheets_update`: `{operation_id:string}`. They commit only the exact authorized preview and enforce its action. No `approved` input.
- `sheets_get_history`: `{operation_id?:string,limit?:number,cursor?:string}`.
- `reports_wedding`: `{spreadsheet:string}`; reports distinguish numeric known costs from unknowns, currency ambiguity and incomplete coverage. Never derive paid balance from budget or funding-source fields.
- Generic `grid` views use a bounded operator-selected range of at most 10000 cells, preserving typed raw values without requiring wedding headers, formulas or domain interpretation. Grid views are read-only; structured Sheets write schemas remain explicit. Model inputs never choose arbitrary ranges.
- Sheets source responses carry spreadsheet ID, actual sheet ID, source range, record identity if established, retrieval time and mapping version. Docs reads carry registered document ID, tab ID, revision/fingerprint, excerpt offsets, retrieval time and mapping version. The XLSX itself is not committed and is never uploaded automatically.
- Reminder tools live in the TypeScript plugin and use verified native `cron.*` gateway RPC. They never implement timing or manipulate native persistence files.
- Unknown fields must be rejected at mutation boundaries. Defaults: 64 KiB request body, 2 MiB provider response, 100 returned Sheets records maximum, 5s bridge/core and 10s provider timeout, at most two Sheets read retries; Docs requests are bounded without automatic retries. Never automatically retry an uncertain write.

Implementation tests should use public tool routes and external HTTP stand-ins. Production dependency availability and live runtime compatibility are separately evidenced.


Rust implementation: HTTP presentation decodes these tool names into typed application commands. The in-process command bus coordinates domain rules through source/journal ports. Concrete Google/SQLite adapters implement those ports; error categories become HTTP statuses only in presentation. The command-bus refactor does not alter this public contract or grant tool-callable operator authorization.

- `sheets_preview_create_tab`: `{operation_id,spreadsheet,title,rows,columns,headers?}`; existing registered spreadsheet only, explicit `create_tabs` permission (default false), global writes required. Title 1–100 characters; rows 1–1000, columns 1–26, total <=10000 cells. Headers <=columns, <=256 UTF-8 bytes each, literal text. Generated sheet ID is persisted in the immutable `sheets_tab` preview (`record_id` and `after.sheet_id`). Full bounds/access/recovery policy is in Workspace capabilities.
- `sheets_create_tab`: `{operation_id}`; same trusted approval hook and shared journal/FSM. One atomic addSheet + optional header batch. No retry after ambiguity; operator reconcile observes only. Result remains unregistered and unreadable/unwritable until separate operator registration; no automatic view or registry mutation.

T027 grid edits reuse `sheets_preview_change`/`sheets_update`: action update, omit record_id, fields `{range,values}`, complete <=100-cell A1 rectangle contained in operator `writable_range`. No permission defaults on. Literal text/safe numbers/booleans/null clearing only; formulas/merges/registered headers/structured-view overlaps protected. Preview uses sheets_grid kind, composite spreadsheetId:sheetId identity and entered-value matrices; same immutable approval/journal/FSM with one send and observation-only reconciliation. Full bounds/concurrency limits are in Workspace capabilities and OpenAPI.

- `docs_search`: `{document:string,query:string,tab_id?:string,limit?:number,cursor?:string}`. Case-sensitive literal/nonoverlapping body-text search in a registered readable Doc; query nonblank <=256 UTF-8 bytes, default10/max50 hits. Snippets include <=80 Unicode scalars each side, with match/snippet offsets and source metadata. Titles capped at200 scalars. Cursor binds exact query, tab selection, document identity, fingerprint and registry version. Read-only; no regex/normalization/Drive discovery; offsets are not Google edit indices.
