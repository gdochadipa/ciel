# Secretary Core

Axum/Tokio service on `127.0.0.1:8787`. All tool calls require the configured business bearer token; its owner identity comes from configuration. The domain/application/presentation/infrastructure layers separate business rules from HTTP, Google transport and SQLite persistence. Legacy views cannot issue provider writes.

Build/test with `cargo test -p secretary-core`; run with `cargo run -p secretary-core`. The production dependency graph includes Reqwest + rustls and jsonwebtoken RS256 service-account assertions. No handwritten signing or replacement HTTP client is used.

Set `SECRETARY_CONFIG` to the registry JSON path, `SECRETARY_CORE_TOKEN` to a random secret of at least 12 bytes, `SECRETARY_OWNER` to the sole owner actor, and `GOOGLE_APPLICATION_CREDENTIALS` to the protected service-account JSON path. The Google client exchanges a one-hour JWT with Google OAuth, caches the short-lived access token with a safety margin, and requests only `spreadsheets.readonly` scope. The credential file is never returned or logged.

Registry loading at startup and in the offline operator CLI uses the same bounded
file decoder. Inputs must be regular files of at most 1 MiB; final symlinks and
duplicate JSON keys at any depth are rejected, including escaped spellings of the
same key. Errors do not disclose paths or configuration values. Startup validates
this registry snapshot before opening Google credentials. This loader does not
check administrator ownership of ancestor directories or implement service-manifest
admission; those read-only service gates remain pending.

For an explicit disposable HTTP simulation only, `SECRETARY_MOCK_MODE=true` selects `SECRETARY_GOOGLE_BASE_URL` and `SECRETARY_GOOGLE_TOKEN` instead of service-account credentials. HTTP is accepted only for loopback stand-ins; other endpoints require HTTPS. URLs and credentials are never tool inputs. Production launches must omit mock mode.

`config/spreadsheets.example.json` is an entirely synthetic fixture, not a production mapping. It uses invented spreadsheet/sheet IDs and some sanitized headers/merge layouts. Before live activation, register the actual Google spreadsheet ID, stable numeric tab IDs, exact headers, *all* intersecting merge ranges and exact formula locations/text from a fresh read-only live inspection. Keep aliases distinct. The live XLSX package IDs must not be reused as Google IDs. JSON is the operator registry format; no new YAML parser is needed. The five legacy kinds have fixed bounded record envelopes. Header/formula/merge drift rejects the whole read instead of guessing. Item positions also require nonempty labels. Unsupported currency and nonintegral/ambiguous amounts remain unresolved.

Views preserve cost-group anchors, raw guest estimates, yearless planning blocks, two column-oriented quote options, and fourteen household positions. Summaries do not infer payments, confirmed vendors, attendance, deadlines or completion. A literal dash remains unknown. Sources include actual Google sheet ID, A1 range, mapping version, timestamp and response fingerprint; `record_id` remains null for legacy positions. Fingerprints are read snapshots, not durable Google revision IDs or write-conflict guarantees. Search is literal and paginated snapshots reject a changed source. Wedding reports deliberately exclude household data.

Provider responses are streamed under a 2 MiB limit with redirects disabled. Each socket has a 10 second bound, and the entire read including OAuth/retries is also bounded to 10 seconds. At most two read retries apply to connection/timeout failures, 429 or 5xx responses; no writes exist here. Errors expose only stable sanitized codes. Bridge requests have a separate 5 second timeout, so a bridge may stop waiting before a bounded provider read finishes.

`services/secretary-core/scripts/check-adapter-offline.sh` builds **the actual domain, application, Axum presentation and SQLite adapter modules** with cached dependencies and executes the committed adapter-seam tests when network dependencies are unavailable. This is deliberately narrower evidence than a production build: it compiles the real router and exercises it with in-process requests, but it does not compile Reqwest/OAuth or execute the socket-based HTTP stand-in tests. The latter remain in `tests/public_api.rs` and must pass in an environment permitting dependency fetch and localhost binds. No credentials or private workbook data are needed for these sanitized tests.

## Explicit read-only foundation

The offline command `secretary-core service check --manifest FILE` now parses the
strict manifest/service policy and inspects a Linux candidate without runtime
effects or secret-body reads. Read the [service-check contract](../../contracts/service-check.md)
for schemas, ownership requirements and sanitized exit statuses. Static completion
returns exit `3` / `readiness: incomplete`: budgets, effective launcher groups,
full package compatibility and runtime-secret validation remain pending.
This command never starts a service or qualifies the host; macOS authority
inspection fails closed. The existing no-argument startup path stays unchanged.

For a candidate registry, use the offline command
`secretary-core registry check --config PATH --profile readonly`. It rejects
configured tab creation, grid write ranges, structured write views/schemas and
writable Docs, including dormant grants. It needs no runtime credentials and
does not start Core, contact Google or attest service readiness. Existing checks
without `--profile` retain their permission rules; all registry inputs now use the
strict shared decoder described above. Unknown profiles fail closed.

Rust embedders may explicitly construct `AppState::new_read_only` or
`CommandBus::new_read_only`. This profile admits only registered Sheets/Docs
list/describe/read/search and their existing summary-context modes. Other decoded
tools and approval commands fail with `readonly_profile`, including full history
and the optional wedding report, before provider access or mutation-intent
creation. Operator reconciliation remains separately authenticated and only
observes; operational journal recovery does not write to Google.

The HTTP constructor requires a Core token of at least 32 bytes. Attaching a
disabled journal coordinator additionally requires a distinct operator token of
at least 32 bytes; an enabled coordinator is rejected. These constructors do not
qualify OAuth scopes, service ownership, host routing or a legacy-journal owner
transition. The executable still uses its existing startup path and full preflight.
The offline checker adds no production profile switch, bounded runner, receipt
promotion or permanent-start authorization.

## Controlled structured writes

T004 adds opt-in existing `AI_Tasks` and `AI_Payments` tables. No tool creates tabs, adds columns, modifies legacy data, or repairs formulas. The synthetic example now includes both table schemas, with `record_id` in A and `operation_id` in B. Registered field columns must be contiguous from C and headers exact; these tables cannot contain merges or formulas. Unregistered fields, identity edits, duplicate row/operation IDs, malformed values and populated unowned columns fail closed. Task status is explicit. Payments require positive exact whole-unit amount, explicit three-letter uppercase currency, absolute payment date and a registered wedding cost-group reference. Dates are never inferred from execution time. Initial structured records require stable IDs and operation IDs; legacy row coordinates cannot be promoted into identity.

Set `SECRETARY_STATE_DIR` to a protected local directory to enable the SQLite journal/history. It creates `secretary.sqlite3` with mode 0600 in a 0700 directory, WAL + FULL synchronous durability, transactional state/audit records and a held OS file lock preventing a second process from recovering a live writer. The SQLite choice is SQLx 0.7, not a handwritten storage engine. In-process writes and approvals are serialized. Symlinked state paths/files are rejected. An unavailable journal prevents write execution; failures and denied inputs are sanitized and audited when the store is available.

Writes remain disabled unless `SECRETARY_WRITES_ENABLED=true`. Enabling them additionally requires a distinct `SECRETARY_APPROVAL_TOKEN` of at least 12 bytes and the state directory. Only this explicit flag requests Google's read/write service-account scope; otherwise the existing read-only scope is retained. The service account must separately have access to the approved existing structured tabs. Never enable this flag in production before live qualification.

A business tool requests an immutable five-minute preview containing configured actor, generated record identity, exact before/after field values, source fingerprint, mapping version and digest. The operator posts `{decision:"approve"|"reject",preview_digest}` to `/v1/operations/{id}/approval` using the separate operator bearer token. It is not a model tool. Rejected/expired previews cannot commit. `sheets_append` and `sheets_update` take only `operation_id`; the action must match. Same-ID/same-input preview calls replay the stored intent and current state, while changed input conflicts.

Before one provider request, the service durably marks `applying`, revalidates structure/unique identity and compares the exact source snapshot. Append writes a new contiguous owned row with RAW input; update touches only approved field cells and the operation-ID cell. After a successful response it rereads and verifies unique IDs and the exact expected field values. Provider 400/401/403/404/422 is a known rejection; transport failures, timeouts and ambiguous server statuses become `uncertain`. There are no automatic write retries. Google has no atomic compare-and-swap for this operation: an external human edit between preflight and mutation can still race, and row movement in that interval can target the wrong physical cells. The exact post-read detects many such outcomes but cannot guarantee recovery of overwritten data. Production activation must accept this residual limitation or retain read-only mode.

An interrupted `applying` operation becomes `reconciliation` on startup. Operator-only `POST /v1/operations/{id}/reconcile` with `{}` performs reads and checks unique `record_id`/`operation_id` plus approved values. It can establish `applied`, report `conflict`, or retain `reconciliation`; it never resends. Missing evidence cannot prove a failed write. A completed operation is returned idempotently; unresolved or rejected operations cannot be blindly recommitted. Bounded `sheets_get_history` returns immutable previews, current states and sanitized events even after writes are disabled, provided the journal is configured. Back up the SQLite database using the documented SQLite-aware backup tool; editable sheet rows cannot rewrite the journal.


## Source layout

- `domain/`: source mappings, deterministic validation/calculation, immutable previews and the operation FSM. No HTTP, provider SDK or persistence dependency.
- `application/`: typed commands, command bus, mutation coordination and source/journal ports. No concrete adapter imports.
- `infrastructure/`: bounded Google/OAuth transport and durable SQLite journal adapters.
- `presentation/`: Axum routes, strict request decoding, Core/operator authentication and error/status responses.
- `main.rs`: environment loading and adapter composition.

HTTP tool names, JSON envelopes and stored operation names remain stable. Internal Rust write construction now keeps credentials in presentation: create `Writes::new(journal, enabled)`, then `AppState::with_writes(writes, approval_token)` validates the distinct operator secret. Application-only callers can dispatch typed `Command`s directly without transport credentials; they are trusted code, not externally callable LLM tools.

Journal evidence storage now upgrades transactionally from inline previews to
separate `operation_evidence` payloads, preserving exact preview bytes/digests,
permanent IDs, outcomes, task/undo links and audit. An older binary cannot read the
upgraded schema; rollback requires the matched pre-upgrade backup. Retained history
keeps its existing shape. A future approved cleanup receipt can mark source evidence
unavailable while history preserves identity/outcome; replay and undo then fail
closed. Missing payload without a valid receipt is a journal error. No runtime
cleanup/removal API is currently exposed, and startup does not remove evidence.
See [task/undo contract](../../contracts/task-approval-undo.md).
