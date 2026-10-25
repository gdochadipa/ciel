# T004 controlled structured writes

Based on verified ancestor `c23e754`, branch `ticket/controlled-writes`, separate worktree. User requested medium effort and no code review. No credentials, live Google access, workbook mutation, tab creation or production activation.

Implemented: explicit typed existing AI_Tasks/AI_Payments schemas; legacy views permanently read-only; field/header/formula/merge/identity validation; immutable digest-bound owner previews; SQLx SQLite durable journal and transactional audit; private modes and exclusive service file lock; separate operator bearer approval; default-disabled writes and read-only default OAuth scope; one serialized RAW append/minimal field update request with stable IDs and operation marker; snapshot conflict detection; exact read-after verification; known failure/uncertain/conflict/reconciliation outcomes; no automatic write retry; startup interruption recovery; operator-only read reconciliation and bounded history. Dates/currency must be explicit; payment link must reference a registered wedding cost group.

Checks actually executed:

- Cached harness compiles the actual production error/registry/projection/write_model/journal modules with SQLx SQLite, not a storage substitute: 11 tests pass (5 read adapter, 3 structured adapter, 3 durable journal). Journal restart/idempotency, rejected/expired approvals, changed payload/digest, private file mode, exclusive writer, yearless/unknown preservation and structured identity/type constraints are exercised. The exclusive-lock test was observed failing before lock implementation, then passing.
- `cargo fmt --all -- --check` passes.
- Updated registry/OpenAPI parse as JSON (valid YAML serialization for OpenAPI).
- Full `cargo test --offline -p secretary-core` remains blocked before compilation: uncached jsonwebtoken/Reqwest production dependencies. The previously established crates.io DNS failure and sandbox localhost-bind prohibition remain unresolved; no blind repeated network probes were attempted.
- `controlled_http.rs` contains real Core HTTP -> real Reqwest -> stateful Google-only HTTP acceptance for accepted-then-ambiguous append, durable approval, owner binding, exact idempotency, explicit reconciliation, no second append and legacy preservation. These full HTTP checks are **committed but unexecuted**, not claimed passing.

Remaining gates: full production compile/all public HTTP acceptance in an environment with dependency access and localhost binds, actual service-account read/write permissions on disposable approved structured tabs, update/preservation/conflict/timeout acceptance against real Google semantics, live SDK/operator callback qualification, operational restart/backup validation, and all T007 activation evidence. No accurate full Cargo.lock can be generated while dependencies are unavailable; none was fabricated.

Residual limitation is explicit: Google Sheets values writes offer no atomic compare-and-swap. A human edit/reorder between preflight snapshot verification and the provider write can race. Minimal RAW updates and exact post-read detection reduce damage but cannot prove atomic conflict exclusion or automatically restore overwritten data. Keep production read-only until this limitation and all live gates are accepted.
