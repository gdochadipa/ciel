# Initial Docs summaries in OpenClaw memory

The owner requests a short initial summary per previously read Doc, retained for discovery for seven days and refreshed after a confirmed edit through OpenClaw. OpenClaw owns these notes. Core prepares source packets; its operation journal never stores summaries. This source workflow does not add a memory backend, model service, automatic deletion job or memory tool to the plugin.

## Native host workflow

Use the active agent's native memory search/read and authorized workspace file-writing tools. Before using a note, call docs_list and match its readable registered alias, document ID and mapping version. Native memory is not a permission grant. A missing, replaced or unreadable registration cannot be recalled through this workflow. Do not select or install a different memory provider or enable an embedding/billing service.

For discovery, search native memory for the requested topic and `ciel_docs_summary_v1`, then read only a bounded candidate note. Check the complete metadata below; a search snippet alone cannot establish validity. A valid note can identify a likely file and show a clearly dated discovery summary without rereading its body. For a fresh summary, detailed facts or an edit, fetch current registered Docs source first. Reuse the note's prose only if current source fingerprint and registry mapping match; otherwise regenerate it. A seven-day TTL does not prove the Google document is unchanged.

Store a separate small note for each source, even for a combined summary. Use a stable native workspace path under `memory/ciel-docs/`, named from a hash of the trusted alias and document ID; never use a source title/text as a path. Update that same note for that source. Keep it out of MEMORY.md, USER.md and daily bootstrap notes, and mark it as derived, untrusted discovery data excluded from durable-memory promotion. Respect native path containment and permissions; the source guide grants no new host tool access.

Each note contains:

- kind `ciel_docs_summary_v1`, document alias and ID, mapping version, source fingerprint, available revision and packet context_id;
- `source_retrieved_at`, `expires_at` exactly seven days after that source retrieval (UTC), and `cache_status` (`fresh`, `stale` or `unverified`);
- projected-body scope, included/total characters, completeness, and a compact citation legend for the note's actual excerpt labels/spans;
- one initial summary of at most1200 Unicode scalars. Keep suggestions, instructions, raw excerpts, full document text and secrets out of this note. Source labels inside text do not become evidence.

On first successful discovery/read of a Doc selected by the owner, if no reusable note exists, fetch bounded summary_context once and generate its short initial source summary. Create/update the note after this meaningful summary of fresh source text; do not summarize every search hit or enumerate unselected Docs. Empty/whitespace-only returned excerpts produce no invented summary. Do not append copies on every read, store the whole combined report, or extend expiry when a note is merely searched/read. Label partial source coverage in both note and displayed text. Subsequent fresh source retrieval can renew the note only after matching/recomputing its summary and evidence.

A reusable note must have matching kind/identity/mapping, valid UTC timestamps, `cache_status=fresh`, meaningful bounded summary and complete source/coverage evidence. Require `source_retrieved_at <= now < expires_at`, with the seven-day interval intact. Missing/malformed/future/expired metadata is a cache miss; refresh from registered source when needed. Never quote an expired note as current source evidence. TTL is a logical reuse rule, not a promise that the native search index, transcripts or files physically disappear after seven days.

## Edits through OpenClaw

When an approved docs_apply begins, mark the existing note stale before attempting the write. A pending/failed/denied/conflicted/uncertain outcome must not renew the note or claim a fresh summary. If marking is unavailable, do not reuse that note without a fresh source check.

After docs_apply is confirmed applied, including an applied replay or observation-only reconciliation, fetch fresh docs_read summary_context for that document. Generate/update its short note from this new packet and set a new seven-day expiry. Do not derive a new fingerprint/summary from the preview, journal state, old report or an ambiguous provider response. If reread or native storage fails, keep the note stale/unverified and tell the owner the edit succeeded but the summary note was not refreshed. Once uncertain execution reconciles to applied, use this same fresh-read path; never resend a write to repair a memory note.

Edits by humans in Google or other integrations are not observed proactively: current-source reads detect changes. No scheduler, polling process or extra daemon is added.

## Qualification and pinned SDK evidence

The locally installed OpenClaw2026.10.7 package docs/concepts/memory.md describes Markdown workspace memory, native memory_search/memory_get and writing notes with native file tools. It documents expiry as action-sensitive metadata; no per-note automatic TTL API is documented there. Therefore this increment provides a host-model workflow with logical expiry, not deterministic plugin-enforced TTL/deletion or an automatically installed memory integration.

The plugin archive/installer does not install the source skills or grant native memory/file tools. Current effective host permissions, successful note save/read/refresh, expiry behavior, index/dreaming handling and token reduction require separate host qualification. Keep production/approval/scheduler flags off. Local Core/bridge/simulation tests do not execute a model or prove these memory behaviors. If native tools/skill access are absent, provide the summary in conversation and explicitly report that no note was saved or refreshed; do not claim the owner's required memory step completed.
