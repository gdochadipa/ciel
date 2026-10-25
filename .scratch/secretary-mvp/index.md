# Secretary MVP implementation graph

Spec: `docs/specs/openclaw-personal-secretary.md`  
Workbook mapping: `docs/spreadsheet-assessment.md`  
User instruction: implement locally, medium-effort agents, no code review. Production changes require the documented activation checks.

| Ticket | Depends on | Status | Outcome |
| --- | --- | --- | --- |
| T001 Foundation and contract | — | done | Local integration repository, interface conventions, confirmed local tracker; commits 70cd886 and 233ba0b. Machine OpenAPI/build manifests belong to T002/T003. |
| T002 Read-only Rust Core | T001 | implemented; acceptance blocked | Implementation 4b4ade1 integrated; five actual-source adapter tests pass. Full Reqwest/OAuth/HTTP build and tests require unavailable dependencies and socket permission. |
| T003 OpenClaw bridge and native workflows | T001 | implemented; acceptance blocked | Implementation b18976b integrated; five tests pass, two HTTP tests skipped. SDK/runtime and native memory lifecycle remain unqualified. |
| T004 Controlled spreadsheet writes | T002 | in-progress | Durable immutable previews, operator approvals, single writer, idempotency, audit and reconciliation. Production writes stay disabled. |
| T005 Skills, CI and operations | T001 | done locally | Implementation 557a9e0 integrated; documented safe CLI smokes and syntax checks pass. Live activation remains T007. |
| T006 End-to-end acceptance and provider simulation | T002, T003, T004, T005 | in-progress; execution blocked | Authoring real tool-to-core runner and upstream Google simulation; actual HTTP execution is blocked by this sandbox. |
| T007 Live activation qualification | T006 | blocked | Actual VPS/OpenClaw/Google/WhatsApp/model entitlement evidence and credentials unavailable. |

T007 is a required production-release gate. No ticket is closed merely because code was written. Optional independent agents and external research providers remain future extensions, as specified; domain guidance and bounded interfaces are included locally.
