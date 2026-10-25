# Secretary MVP implementation graph

Spec: `docs/specs/openclaw-personal-secretary.md`  
Workbook mapping: `docs/spreadsheet-assessment.md`  
User instruction: implement locally, medium-effort agents, no code review. Production changes require the documented activation checks.

| Ticket | Depends on | Status | Outcome |
| --- | --- | --- | --- |
| T001 Foundation and contract | — | in-progress | Local integration repository, contract conventions, tracker, build layout. |
| T002 Read-only Rust Core | T001 | todo | Authenticated bounded Sheets registry/read/search, five workbook projections, deterministic reporting, real Google client. |
| T003 OpenClaw bridge and native workflows | T001 | todo | Owner-bound tools, bounded core client, trusted approval flow, native cron lifecycle, explicit memory policy. |
| T004 Controlled spreadsheet writes | T002 | todo | Durable immutable previews, operator approvals, single writer, idempotency, audit and reconciliation. |
| T005 Skills, CI and operations | T001 | todo | Five domain Skills, secure config examples, systemd/deployment/backup guidance, CI and local verification scripts. |
| T006 End-to-end acceptance and provider simulation | T002, T003, T004, T005 | todo | Real tool-to-core requests, Postman upstream HTTP simulation, restart/adversarial checks, truthful evidence. |
| T007 Live activation qualification | T006 | blocked | Actual VPS/OpenClaw/Google/WhatsApp/model entitlement evidence and credentials unavailable. |

T007 is a required production-release gate. No ticket is closed merely because code was written. Optional independent agents and external research providers remain future extensions, as specified; domain guidance and bounded interfaces are included locally.
