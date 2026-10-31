# Code and comments

Mandatory repository rules. More specific mutation, memory and processing rules
are in [fsm.md](fsm.md), [memory.md](memory.md) and [processing.md](processing.md).

## Ownership

- Keep one Rust Core, one directly awaited in-process command bus and one durable
  operation FSM. Add no broker, separate CQRS buses, projection worker or daemon
  without an explicit revised architecture decision.
- Domain owns validation, immutable preview rules and legal transitions.
  Application coordinates use cases through ports. Infrastructure implements
  provider/storage ports. Presentation authenticates, decodes and maps responses.
  `main.rs` composes them. Domain/application must not import Axum, Reqwest, SQLx
  or concrete infrastructure implementations.
- Give functions/modules clear responsibilities. Separate domain decisions,
  provider access and persistence instead of combining them in generic helpers.
  Review responsibilities rather than imposing arbitrary line-count limits.
- Validate external inputs at boundaries. Propagate explicit errors. A default
  must be defined by the contract; fallback values must not conceal failures.

## Comments and compatibility

- Use English for code, comments and technical rules. Use names and types to
  explain execution steps; comments explain rationale, invariants and risks.
- Place concise comments beside non-obvious write-dispatch, persistence, recovery,
  security and compatibility decisions. Do not require comments on every function
  or mandatory comment tags.
- Explain compatibility constraints locally. Persisted Preview fields named
  `spreadsheet`/`view` also represent Docs document/tab targets to preserve journal
  JSON and digests. A rename needs deliberate compatibility handling.
- Put essential rationale in the code comment itself. Optional references must
  not make committed code depend on unpublished local documentation.

## Verification and adoption

- Use the existing formatter/linter/type-check/test toolchain. Integrate FSM matrix
  and layer-dependency checks into `scripts/verify.sh`; violations must fail checks.
  These additional checks are required adoption work, not already verified features.
- Naming clarity, responsibility boundaries and comment rationale require review.
  Automated success does not prove these review obligations were met.
- Start adoption in FSM/mutation/approval/journal/recovery; apply rules elsewhere
  when changing that code. Preserve journal formats, digests and public behavior
  during initial adoption.
- Keep transition tables, code and tests aligned. Treat discrepancies as defects;
  do not silently choose whichever version makes a change easiest.
- Do not weaken a rule, remove tests or add a bypass merely to pass verification.
  Present exceptions with the affected rule, reason, risks and alternatives for
  owner decision; record accepted scope in versioned rules. Continue independent
  work while a required decision is pending.
