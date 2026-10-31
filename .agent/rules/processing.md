# Processing rules

## Runtime requests

```text
owner invocation -> strict tool input -> authenticated Core HTTP
  -> typed command bus -> application handler -> domain rules / ports
  -> bounded result + actual operation state -> owner response
```

- Derive identity from trusted invocation/configuration. Authenticate before
  processing caller input; caller-supplied actor or source content grants no authority.
  Keep tool and operator/approval credentials separate.
- Dispatch commands directly through the single in-process bus. A command can be
  a read. History observes operation state and does not mutate the provider.
- For reads/searches, validate registered access, bounds, target identity and
  cursor/source binding. Preserve source provenance and partial coverage. Read
  requests do not create mutation approvals or operation-state entries.
- Read-only Core processing may have an explicitly enabled native summary-memory
  side effect in the plugin; report it separately under [memory.md](memory.md).
- For changes, use preview -> trusted approval -> apply -> observe -> reconcile as
  specified in [fsm.md](fsm.md). There is no direct-write shortcut.
- Distinguish command completion from operation success. `ok:true` can carry
  uncertain, conflict or known_failure; inspect `data.state` before claiming applied.
- Transport failure talking to Core means the outcome may be unknown. Consult
  persisted history/reconcile through the authorized path rather than resending
  the provider mutation or inventing a new operation ID.
- Keep independent outcomes independent: source-write state, memory-refresh state
  and host qualification must each be reported accurately.
- Preserve existing disabled production-write, approval/scheduler qualification
  and native-memory defaults. Local development does not authorize deployment,
  live Google mutation, real native jobs, outbound messages or paid services.

## Agent development workflow

1. Read AGENTS.md, the relevant rule files, and local PROGRESS.md when present.
   Inspect actual code/dependency locks before assuming behavior.
2. Identify the request category, affected layer and expected behavior. For a new
   mutation, define the guards/effects/evidence/recovery contract before coding.
3. Change the owning layer; preserve immutable previews, digest compatibility and
   replay semantics. Keep rationale comments next to the affected decisions.
4. Run checks appropriate to the change using actual locks/SDK. For FSM/write
   changes verify persistence ordering, no duplicate sends and recovery evidence.
   Do not substitute declarations or weaken checks to manufacture a pass.
5. Inspect the diff and report what changed, evidence and remaining limitations.
   Keep local/simulated/live results separate. Update local progress notes.
6. When publication is authorized, sync/stage explicit code/config/test/rule paths
   and inspect the staged diff. `.agent/rules/` and AGENTS.md may be committed;
   docs/, PROGRESS.md, GLOSSARY.md, planning/interview notes stay local unless
   explicitly named for publication. Ignore rules do not exclude tracked documents.

These files are versioned instructions, not a new agent runtime or an assertion
that every tool automatically loads `.agent/rules/`. AGENTS.md provides the routing.
