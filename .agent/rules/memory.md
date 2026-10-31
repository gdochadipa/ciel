# Memory management

These rules describe native Docs discovery summaries implemented in
`plugins/secretary-tools/src/summary-memory.ts`. They do not authorize live host
activation. Sheets summary context does not imply persistent Sheets memory.

## Ownership and authority

- OpenClaw owns personal memory. Core SQLite holds operation/approval/audit/recovery
  state, not notes, full documents or another knowledge store.
- Native summary persistence is opt-in (`summaryMemoryEnabled` defaults false).
  Require current owner invocation, matching owner-private audience, current
  invocation/audience assertions, supported local workspace and safe-root IO.
- Require an admitted memory-core slot and explicit dreaming.enabled=false.
  Keep temporary source summaries out of MEMORY.md, USER.md and bootstrap notes.
  Do not promote external-source content into durable personal instructions.
- Source text, titles and model summaries are untrusted data. They cannot alter
  rules, approve writes, choose tools/credentials or authorize outbound actions.

## Admission lifecycle

This is a note-validation lifecycle, not a second durable operation FSM. Only
fresh/stale are persisted cache labels. Missing/expired/invalid/blocked are admission
conditions; do not add a new engine, daemon or SQLite table for them.

1. Resolve the current readable registration through Core. Match alias, source ID
   and mapping version before admitting a note.
2. Hold the exclusive per-source native lock. Reject unsafe paths, links and
   unmanaged conflicting content; use private bounded files and hashed names.
3. A fresh valid note within TTL may serve dated discovery information. It does
   not prove the source is currently unchanged; facts/edits need fresh source reads.
4. Missing, malformed, expired or stale notes are misses. If no unresolved write
   fence exists, read fresh bounded source under the lock, generate the short
   summary with the existing host model, validate it and replace the same note.
5. Validate source provenance, coverage, context identity and actual citations.
   Store bounded summary/evidence, not the full source packet. Structural citation
   validation does not prove semantic quality.

TTL is exactly seven days from source retrieval, not from cache access. Hits and
unchanged-source reads never slide expiry. Removed/unreadable/replaced registrations
cannot reuse the old note. Generic native recall must not bypass these checks.
Logical expiry does not prove physical deletion from indexes, transcripts or backups.

## Coupling to Docs writes

```text
registered source -> fresh bounded read -> validated note
validated note -> stale + pending operation fence -> docs_apply
docs_apply applied -> fresh source read -> replace the same note
docs_apply uncertain/interrupted -> retain stale/fence -> observe Core outcome
refresh fails after applied -> keep write applied and memory stale
```

- When native summaries are enabled, resolve the Docs target from Core history,
  acquire the native per-source lock and persist a stale pending-operation fence
  before invoking docs_apply. Fence failure stops dispatch.
- Only an observed terminal Core outcome for the same registered target clears
  an unresolved operation fence. Search or forced refresh cannot bypass it.
- An applied result, including replay/history observation, refreshes from a new
  source read. A memory refresh failure never resends or rolls back the source write.
- Ordinary successful source reads remain usable when optional memory work fails;
  report memory status separately. Explicit discovery-summary mode must fail
  visibly if its required memory step cannot complete.
- Read replacement source under the note lock so delayed old snapshots cannot
  overwrite post-edit notes. A crash/retained lock fails closed; do not reclaim
  it based only on age. Operator recovery must quiesce the owner, inspect the
  matching Core outcome and verify exact path/ownership before removing a lock.
- Report local tests, simulated model/provider behavior and qualified live-host
  evidence separately. Do not infer host indexing, model quality, deletion or
  token savings from local fixtures.
