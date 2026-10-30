# Native Docs discovery summaries

T038 implements short per-source summary notes in the active agent's OpenClaw workspace memory. Core still owns registered-source access and operation state; its SQLite journal stores no summaries. No memory provider, database, embedding service, model credential, background process or scheduled cleanup job is added.

## Use and host policy

The plugin option summaryMemoryEnabled defaults false. An operator can enable it for a qualified local host; the plugin does not edit host configuration. Keep production writes and approval/scheduler qualification flags off until their separate gates pass.

Native access requires a current owner invocation, host-selected owner-private memory audience matching the agent, current audience/invocation assertions, a local absolute agent workspace and sandboxed=false. Remote/declared workspace bindings and incompatible filesystem roots are rejected rather than redirected to a local copy. Configured memory-core dreaming.enabled must explicitly be false and the selected memory slot must not be disabled or replaced. This avoids silently promoting temporary external-source summaries into durable memory; the feature does not alter that policy or install a memory provider.

Call the existing plugin tool:

```json
{"document":"notes","mode":"discovery_summary"}
```

Only document, mode and optional boolean refresh are accepted. This mode belongs to the host plugin; Core's docs_read contract still handles legacy reads and summary_context. No caller-supplied path, summary text, model, TTL or credential is accepted.

The plugin first asks Core docs_list for the current readable alias, document ID and mapping version. It then validates the native note. A hit returns a dated short discovery summary, source/citation/coverage evidence and expires_at without a Google body read or another model completion. This can help identify a previously read file; it does not establish that Google contents remain unchanged. For fresh summaries, detailed facts or edits, use current source reads. Force a refresh with:

```json
{"document":"notes","mode":"discovery_summary","refresh":true}
```

A miss or forced refresh reads a fresh bounded single-Doc summary_context and calls the existing host runtime.llm.complete with the active model reference, or that agent's configured model when no active reference is provided. No alternate provider/credential/model fallback is configured by the plugin. Model output is bounded JSON with a <=1200-Unicode-scalar summary and1–5 actual structured excerpt IDs. The plugin validates IDs/ranges and their labels before saving. This structural check does not establish semantic model quality.

## Native records and expiry

The SDK's native safe-root helpers publish a single private0600 Markdown note under memory/ciel-docs; the child directory is private, and an existing parent memory directory is preserved. Names are hashes of registered alias/document ID, never source titles or model text. The folder is part of native OpenClaw memory, not a second plugin knowledge store. Symlinks, hardlinks, unbounded files and unmanaged conflicting content are rejected.

Notes contain kind ciel_docs_summary_v1, status, short initial prose, source identity/fingerprint/revision/mapping/retrieval time, context_id, exact selected citation spans, projected-body coverage and expiry. They do not contain the raw source packet, full text or combined report. They are untrusted derived discovery data, not directives or approval. Keep them out of MEMORY.md, USER.md and daily bootstrap notes.

Expiry is exactly seven days after source retrieval: source_retrieved_at <= now < expires_at. Hits never slide it. Missing, malformed, future, expired or stale notes are misses; removed/unreadable/replaced registrations cannot reuse old notes. Refresh overwrites the same source's note, rather than appending duplicates. Ordinary successful Docs reads/searches also create/update the initial per-source note. Unchanged-source reads reuse it without renewing expiry or calling the model again. Empty source data cannot invent a summary.

Native persistence status is a separate text item in the SDK tool result (summary_memory). Original Core source/history JSON remains unchanged in details and the first text item. Missing native authority, policy, model or storage is reported as unavailable; it does not turn an ordinary successful source read into a claimed memory save. Discovery mode itself fails visibly when its required memory step cannot complete.

TTL is enforced by this plugin's discovery admission path. It does not physically delete notes, transcripts, backups or index snapshots, or impose a TTL on generic memory_search/memory_get outside this path. Their results must not bypass expiry/current-registration checks. Native indexing/recall/taint behavior and actual token savings remain host qualification; the sample host keeps dreaming and automatic memory ingestion disabled.

## Writes, uncertainty and recovery

When enabled, docs_apply resolves its actual target from shared Core operation history and marks a native tombstone stale before invoking the single provider-write path. The tombstone records the pending operation ID, not source text. Failure to obtain the native per-source fence stops the write before provider dispatch. Existing trusted approval/digest/FSM checks remain authoritative.

After applied state (including replay), the plugin fetches current source and refreshes that same note with a new seven-day expiry. Refresh failure is reported separately: the source write remains applied, while the note remains stale. The plugin never resends a source write to repair memory.

Uncertain/applying/reconciliation outcomes cannot be refreshed merely by searching or forcing refresh. The stored operation ID must first be observed in a terminal Core state for the same registered Docs target. An applied history observation performs the fresh-source refresh; no provider write is repeated. Ordinary fresh source reads may still be returned while memory remains unavailable/fenced. Human/external Google edits are detected by subsequent fresh source reads, not proactive polling.

Native reads, refreshes and applies use an exclusive per-source private lock across independent SDK factories/processes. Every replacement note uses a fresh Core source read under that lock, so a delayed pre-edit response cannot revert a post-edit note. Ordinary source JSON still represents its own captured snapshot. A retained/crash lock fails closed; there is no blind timeout-based reclamation. Operator recovery must quiesce the owner, inspect the corresponding Core operation outcome and verify the exact native source path before removing a retained lock. Do not delete journal state or blindly repeat a write to clear it. Revoked invocations cannot publish notes; failed cleanup can intentionally retain the fence.

## Local evidence and remaining qualification

Tests use the actual locked SDK bundle on its supported Node24 runtime and real native file IO in disposable workspaces. The model completion boundary is deterministic test output, not a live model. Factory tests cover persistence/recreation, seven-day boundary, no sliding/duplicate writes, current registration, strict input/owner/audience/lifetime/policy checks, native symlink/lock rejection, invalid model output, failed refresh and observed reconciliation.

Workspace acceptance runs real Node/SDK/native file IO and Rust Core/journal with Google and model responses simulated, including one applied edit and accepted-write/lost-response reconciliation. It does not qualify target VPS installation, effective live authority/skill loading, real model prose/citations, native search indexing, physical erasure or measured token use. No user home, credential or personal workbook is mounted. Existing source skills are not automatically installed by the plugin archive.
