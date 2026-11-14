# Trusted read-task accounting v1

One operator-created active read task exists per exact protected owner/deployment
generation. Only the distinct operator bearer creates, closes or increases it.
Business/tool input contains no task selector, lifecycle command, reset or quota
increase. All reservations use server-created unpredictable IDs, bound to that
active task and exact one of the eight read tools. Reads create no mutation
operation or approval. The existing journal gains only operational counters and
reservation states; it never stores source packets, model prose, tokens or keys.

## Protected policy and units

`ciel.readonly.budget-policy.v1` contains required `schema`, `revision`, `ceiling`
and `request_output_bytes`. `ceiling` has required integer `calls`, `rows`,
`document_characters`, `output_bytes`, `in_flight`; no field defaults. Task limits
use that same limits object and must be within the protected ceiling. Calls are
1–10000, rows 0–1000000, document characters 0–100000000, total output bytes
512–67108864, in-flight 1–16 and no greater than calls. Request output bytes are
256–1048576 and must fit total bytes minus 256. Unknown/duplicate fields deny.
The example is an explicit illustrative ceiling, not tuned/default product limits.

- Calls: each durably admitted bridge invocation, before local validation and IO.
  Bad input, failures, cache/repeated reads, retries and successive turns consume
  calls. HTTP reservation/settlement control messages are not additional tool calls.
- Rows: each returned entry in Sheets `records`/`rows` arrays, including repeated
  pages. Other Sheets metadata is bounded by whole-output bytes.
- Document characters: Unicode scalars across all returned Docs string values,
  conservatively including document metadata/citations, not only body excerpts.
- Bytes: the complete exact UTF-8 JSON string returned in plugin text content,
  including data, metadata, citations and error envelopes. No byte-to-token or
  spending-cap claim. Provider traffic, host schemas/transcripts, final host model
  answers, input/output/cached model tokens and billing remain separate evidence.
- Every admitted invocation charges at least 256 output bytes. Task creation
  charges/reserves 256 bytes for one bounded stop notice. These conservative floors
  cover a lost/invalid Core response replaced by one short bridge error. Counters
  report charged bytes, not a claim that every charged byte reached the model.

SDK/schema rejections that never invoke this plugin execute no plugin processing;
this service cannot count unrelated host activity. In the new read-only bridge,
no text is emitted without a durable grant, except the one pre-reserved task stop
notice. No active task and repeated exhausted-budget attempts return empty content
with bounded UI diagnostics. This prevents an unbounded stream of plugin budget
errors from growing model text after the admitted budget is exhausted.

## Reservation, failure and restart

Reserve before validation/provider access using a SQLite transaction that obtains
the writer lock before reading counters. Reserve at most 100 remaining rows for
Sheets read/search, up to 1048576 remaining document characters for Docs calls,
and the smaller of remaining bytes and protected per-invocation bytes. Count
pending/executing reservations against concurrency and remaining axes. Metadata
calls reserve zero rows; Sheet calls reserve zero document characters. No quota
axis can be overrun by simultaneous calls.
The bridge may lower a Sheets request's normal page limit to the reserved row
allocation. Core still produces its normal source-bound cursor/coverage; no
result evidence is truncated or rewritten, and the original tool input is intact.

`pending -> executing` claims the exact tool once. A local-validation error claims
and settles the pending reservation before any tool request. These paths race
through the same one-time claim; neither can send a duplicate read for that grant.
Settlement validates actual units, charges actual/floored bytes and releases unused
reservation. Duplicate claim/settlement, a different tool/scope and overrun deny.
Oversized source results are withheld as a complete bounded error, never silently
truncated. Existing hard source/request/page bounds remain in force.

No automatic refund on cancellation/timeout or lost response. Reservations that
remain pending/executing conservatively retain their whole allocation. After a
real journal restart, mark orphan reservations lost and charge the full reserved
units; subsequent calls/turns cannot reset them. A journal object can attach its
budget runtime only once. Same-generation policy changes deny; an administrator
must use the explicit deployment/lifecycle procedure. Closing is denied while
reservations are outstanding. Increases are monotonic, within the protected
ceiling, and preserve used counters. They grant no write or approval authority.
A changed authorized limit may re-arm a spent stop notice only by reserving and
charging another 256 bytes within the increased budget. Identical repeated
increase requests neither reset notices nor charge them again.

## HTTP and bridge

The business bearer uses `/v1/read-budget/reserve` with `{tool}`, receiving a
durable reservation or a bounded/empty stop notice. A tool request carries the
server reservation in the bridge-created `x-ciel-read-reservation` header, never
tool input. Local validation failures use `/v1/read-budget/local-error` with the
same reservation/tool; the Core computes its own bounded error and usage.

Budgeted tool responses use `application/vnd.ciel.read-budget+json`, have
`x-ciel-budgeted-result: v1` and carry a result plus
its exact `model_text`. The bridge verifies their semantic equality and byte bound,
then returns that exact string instead of reserializing JS numbers/Unicode. On a
post-dispatch failure it returns one <=256-byte error and never resends the read.

Only the operator bearer may use `/v1/read-budget/operator/{create,increase,close,status}`.
Create/increase require the limits object; close/status require `{}`. Neither
business tools nor caller-provided owner/generation/task IDs authorize lifecycle.
Operator control/recovery output is not model-tool output. Mandatory operation
reconciliation retains its independent authentication and is never stopped by
model read quotas. Read-only profile still rejects preview/approval/apply.

`readOnlyRuntime` is opt-in/default false in the plugin. It disables optional
summary-memory/model completions, reminders and all non-read execution. The legacy
bridge/profile remains unchanged. Neither this flag nor task creation qualifies a
host, grants service admission or activates the manifest route. Full release,
promotion/evidence and target-specific qualification remain required.
