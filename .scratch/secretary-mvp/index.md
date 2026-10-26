# Secretary MVP implementation graph

Spec: `docs/specs/openclaw-personal-secretary.md`  
Workbook mapping: `docs/spreadsheet-assessment.md`  
User instruction: implement locally using medium-effort agents; subsequently review at low effort and fix the findings. Production changes require the documented activation checks.

| Ticket | Depends on | Status | Outcome |
| --- | --- | --- | --- |
| T001 Foundation and contract | — | done | Local integration repository, interface conventions, confirmed local tracker; commits 70cd886 and 233ba0b. Machine OpenAPI/build manifests belong to T002/T003. |
| T002 Read-only Rust Core | T001 | implemented; acceptance blocked | Implementation 4b4ade1 integrated; five actual-source adapter tests pass. Full Reqwest/OAuth/HTTP build and tests require unavailable dependencies and socket permission. |
| T003 OpenClaw bridge and native workflows | T001 | implemented; acceptance blocked | Implementation b18976b integrated; fourteen tests pass, three HTTP tests skipped. SDK/runtime remains unqualified; personal memory was subsequently delegated to OpenClaw. |
| T004 Controlled spreadsheet writes | T002 | implemented; acceptance blocked | Implementation 47d3c68 integrated. Eleven combined actual-source adapter/journal tests pass; full production HTTP/OAuth compilation and provider write tests are unexecuted. Production writes stay disabled. |
| T005 Skills, CI and operations | T001 | done locally | Implementation 557a9e0 integrated; documented safe CLI smokes and syntax checks pass. Live activation remains T007. |
| T006 End-to-end acceptance and provider simulation | T002, T003, T004, T005 | authored; execution blocked | Implementations daa1ad5 and 6211111 integrated. Real tool-to-Core read/write/approval/reconciliation runner and Google-only stateful simulations exist. Actual execution failed before listening with EPERM; no end-to-end pass. |
| T007 Live activation qualification | T006 | blocked | Actual VPS/OpenClaw/Google/WhatsApp/model entitlement evidence and credentials unavailable. |
| T016 Pending-response authority and isolated HTTP | T003, T006, T015 | implemented; fixture HTTP passed in isolated Linux | Revoked pending Core/native responses are denied; two regressions pass. Isolated container runs 34 checks with no skips, including four HTTP stand-ins; actual Core/SDK/provider acceptance remains blocked. |
| T015 Native memory ownership and verification gates | T003, T006, T008 scope decision | implemented; host acceptance blocked | Removed memory placeholder; plugin owns 17 business/reminder tools. Qualified verify now includes host validation, HTTP bridge checks and acceptance configuration; source CI enables HTTP checks. |
| T014 Pinned Gateway and memory safety defaults | T003, T008, T011 | implemented locally; host acceptance blocked | Await native Gateway availability before scheduler access; two public regressions pass. Dedicated-runtime template disables documented automatic memory paths and broad tool access; native lifecycle was subsequently delegated to OpenClaw in T015. |
| T013 Complete approval presentation | T003, T006 | implemented; host acceptance blocked | Full captured preview in approval description, action binding and Core refusal propagation. Six fetch-boundary checks pass; actual approval routing/rendering remains T007. |
| T012 Plugin discovery and packaging | T003, T011 | implemented; host acceptance blocked | 18 tool declarations, metadata validation, distribution allowlist and source CI. Three new checks pass; full host build/validation remains blocked. |
| T011 Plugin reminder reliability | T003, T009 | implemented; native acceptance blocked | Bounded paged discovery, revision-checked updates, stale-invocation guard and native-route drift checks. Fourteen bridge checks pass; installed SDK/native scheduler qualification remains T007. |
| T010 Rust DDD and hexagonal refactor | T002, T004, T009 | done locally; full acceptance blocked | Domain/application/presentation/infrastructure, typed commands, source/journal ports and operation FSM. 16 cached-source checks pass; production Google/OAuth and socket qualification remain blocked. |
| T009 Review fixes | T003 | done locally | Default backend aligned to 8787; canceled/completed native jobs retained for history. Three regressions pass; native activation remains T007. |
| T008 Native personal-memory ownership | T003 | delegated to OpenClaw by owner | Custom memory lifecycle removed from plugin scope; no native lifecycle acceptance claimed. Plugin memory placeholder removed in T015. Native host privacy/permissions remain deployment concerns. |

T007 is a required production-release gate. No ticket is closed merely because code was written. Optional independent agents and external research providers remain future extensions, as specified; domain guidance and bounded interfaces are included locally.

See `docs/implementation-status.md` for the combined evidence and remaining scope. The spec is not fully accepted and the implementation is not production-ready. The subsequent low-effort review and its local fixes are recorded in T009.
