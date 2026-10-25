# T002 read-only Rust Core evidence

Implemented locally on `ticket/read-only-core`, based on verified ancestor `233ba0b`. No code review performed, per user instruction. No production connection or workbook mutation.

Implemented: authenticated Axum tool routes, configured owner actor, typed bounded JSON registry, list/describe/read/literal search, source-bound pagination, the five explicitly bounded legacy projections, deterministic known-cost report, merge-anchor deduplication, unknown dash/currency/payments/dates, reversible explicit `k` text parsing, unclassified E-column/household annotations, formula numeric-input/cache comparisons, precise sources, fail-closed headers/merge/formula drift, real Reqwest/rustls Google API reads, jsonwebtoken RS256 service-account OAuth, bounded response stream/timeouts/read retries, explicit disposable provider mode, and read-only default denial of all mutation/history tools. Machine OpenAPI and synthetic registry are included.

Verification actually run:

- `cargo fmt --all -- --check`: passes after formatting.
- Adapter-only cached-dependency harness over **actual production source**: five adapter seam tests pass. This covers 46 wedding item positions, four shared children counted once, unknown dash states, fourteen household records, yearless task candidates and exact source refs, drift rejection, independent registered SUM checks, reversible text amounts and preservation of unlabeled amounts. New SUM and annotation behaviors were observed failing before their implementation and then passing.
- `cargo test --offline -p secretary-core`: blocked before compilation because `jsonwebtoken` is absent from the local cache. Reqwest is also not locally cached. No dependency substitution was made.
- `CARGO_NET_RETRY=0 cargo test -p secretary-core`: blocked fetching crates.io `config.json`, DNS could not resolve `index.crates.io`.
- Public-route tests with external HTTP stand-ins are committed, including auth/actor rejection, source pagination, bounded inputs, literal search, schema drift and transient upstream retry behavior. They are **not executed/passing evidence**: dependency resolution is blocked; the root also independently confirmed this sandbox forbids localhost socket binds.
- JSON OpenAPI and registry parsed successfully using Python. Full production Cargo.lock cannot be accurately generated until dependencies resolve; none was fabricated.

Remaining acceptance: full production compilation and all HTTP/OAuth tests in a bind/network-capable environment; credentialed live Google structural inspection and read-only service-account access; exact current production registry; currency and ambiguous workbook meanings remain unknown. T002 implementation is delivered, but full acceptance is blocked by environment and live evidence. The example registry is deliberately synthetic and must not activate the real workbook.

Reproduce narrow adapter checks with `services/secretary-core/scripts/check-adapter-offline.sh`; it explicitly excludes the full HTTP/OAuth graph. Run `cargo test -p secretary-core` for real production/public boundary checks once dependencies and binds are available.
