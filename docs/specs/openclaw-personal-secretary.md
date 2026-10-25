# OpenClaw Personal AI Secretary — Architecture & Delivery Specification

Version: 1.2 implementation baseline  
Date: 2026-10-25  
Status: Local implementation authorized; live activation qualification pending  
Delivery: Local specification, as requested; no tracker issue published  
Target: Sumopod VPS, Ubuntu 24.04 LTS  
Development: macOS Apple Silicon

## Problem Statement

The owner needs a personal secretary that can retrieve and organize existing information, record approved changes, manage reminders, and report progress through WhatsApp. Wedding preparation is the first concrete domain. Existing spreadsheets contain valuable information and must remain usable without replacement, silent restructuring, or formula damage.

Conversation history is insufficient as an authoritative record of budgets, payments, vendors, or deadlines. A helpful answer must identify its sources, distinguish uncertainty from missing data, and accurately report each side effect. Model costs, access to personal information, and operational complexity must stay bounded.

The supplied master plan defines the intended system. The supplied workbook has now been inspected read-only: it contains five visible tabs with irregular budget sections, guest estimates, monthly notes, prewedding comparisons, and household purchases. The workspace contains planning documents and the workbook, but no application code, Git metadata, glossary, ADRs, contracts, or test conventions. Local workbook evidence supports the mapping proposal; live Google integration and VPS runtime compatibility remain unverified.

## Solution

Start with one Secretary Agent in OpenClaw. Domain Skills guide its behavior; explicitly authorized Tools perform operations. A small TypeScript Plugin bridges agent tool calls to an authenticated Rust Core Service. The Core Service owns spreadsheet mapping, validation, deterministic calculations, write control, audit records, and recovery. Google Sheets remains authoritative for spreadsheet-managed records. OpenClaw's native scheduler owns reminder execution; channel evidence determines delivery status.

The first deliverable is a read-only discovery and architecture package. Inspect the workbook, identify its relationships and quality issues, map supported records without changing their source layout, and verify the deployed integration capabilities. Present the assessment, decisions, risks, and phased acceptance criteria for review. Do not implement application services, create production tabs, connect WhatsApp, or deploy changes before that review.

This specification covers the intended MVP and its later roadmap. After receiving the workbook assessment, the user explicitly requested implementation, confirmed local Markdown tickets, requested medium-effort implementation agents, and deferred code review. That later instruction authorizes local implementation; production writes, native runtime activation, WhatsApp connection, and VPS deployment remain subject to the affected capability checks. The user-confirmed testing boundaries remain in force.

## User Stories

1. As the owner, I want to use WhatsApp as my main conversation channel, so that I can manage personal information from my phone.
2. As the owner, I want access limited to my approved number, so that strangers cannot retrieve or change my information.
3. As the owner, I want groups disabled initially, so that personal context remains private.
4. As the owner, I want the assistant to use existing spreadsheets, so that I can retain my current workflow.
5. As the owner, I want every existing workbook tab inspected before changes are proposed, so that hidden dependencies are considered.
6. As the owner, I want formulas, formatting, validations, and protected regions identified, so that automation preserves workbook behavior.
7. As the owner, I want quality findings tied to precise source locations, so that I can review corrections.
8. As the owner, I want proposed cleanup separated from applied changes, so that discovery does not alter my data.
9. As the owner, I want to list approved spreadsheets, so that I know which sources the assistant can access.
10. As the owner, I want tabs, headers, and supported fields described, so that I understand each source's capabilities.
11. As the owner, I want records searched and filtered, so that I can find relevant information quickly.
12. As the owner, I want answers to include source references and retrieval times, so that I can verify them.
13. As the owner, I want unknown or ambiguous schemas rejected safely, so that the assistant cannot guess where to write.
14. As the owner, I want each spreadsheet to have independent read and write permissions, so that access matches its sensitivity.
15. As the owner, I want column aliases mapped to consistent concepts, so that natural-language requests work across different layouts.
16. As the owner, I want duplicate names and missing identifiers surfaced, so that the assistant requests clarification before updating a record.
17. As the owner, I want to preview the exact proposed changes, so that I can approve an understandable result.
18. As the owner, I want approval tied to that preview, so that a later or altered request cannot reuse my approval.
19. As the owner, I want expired or rejected approvals to prevent writes, so that control remains with me.
20. As the owner, I want approved records appended without overwriting existing rows, so that new entries preserve history.
21. As the owner, I want updates restricted to approved fields, so that unrelated cells remain unchanged.
22. As the owner, I want repeated requests to avoid duplicate entries, so that network retries do not duplicate payments or tasks.
23. As the owner, I want edits made since a preview detected where possible, so that stale information is not silently overwritten.
24. As the owner, I want uncertain write outcomes reported explicitly, so that a timeout is not mistaken for failure or success.
25. As the owner, I want every attempted mutation recorded durably, so that I can inspect what happened.
26. As the owner, I want change history kept outside editable planning tabs, so that spreadsheet content cannot rewrite the audit record.
27. As the owner, I want failed or incomplete operations reconciled, so that retries and recovery remain safe.
28. As the owner, I want recovery previews to consider intervening edits, so that restoring an earlier value does not destroy newer information.
29. As the owner, I want preferences remembered only when explicitly saved, so that casual conversation does not become a permanent fact.
30. As the owner, I want saved preferences searched, updated, archived, and traced through history, so that memory stays useful and correctable.
31. As the owner, I want budget and vendor facts retrieved from their sources, so that conversation memory cannot override current records.
32. As the owner, I want sensitive facts handled with explicit consent, so that unnecessary personal information is not retained.
33. As the owner, I want one-time and recurring reminders, so that I can manage deadlines conversationally.
34. As the owner, I want reminder dates resolved in Asia/Makassar by default, so that delivery matches my local time.
35. As the owner, I want ambiguous dates clarified before scheduling, so that reminders fire when intended.
36. As the owner, I want reminders listed, rescheduled, paused, resumed, and canceled, so that changing plans remain manageable.
37. As the owner, I want stable reminder and native job identifiers, so that I can inspect and manage the correct job.
38. As the owner, I want execution and delivery tracked separately, so that a running job is not reported as a delivered message.
39. As the owner, I want jobs to survive restarts, so that reminders do not depend on an active chat session.
40. As the owner, I want spreadsheet reminder metadata reconciled with native jobs, so that edits or failures do not create a second scheduling authority.
41. As the owner, I want a deposit entry and its reminder reported separately, so that partial success is actionable.
42. As the owner, I want the Wedding Planner Skill to use my actual wedding workbook, so that it extends my existing preparation process.
43. As the owner, I want expenses, deposits, outstanding balances, and vendors linked where supported by the data, so that financial summaries are understandable.
44. As the owner, I want incomplete financial inputs surfaced, so that unknown amounts are not treated as zero.
45. As the owner, I want preparation tasks and upcoming deadlines prioritized, so that I can focus on the next useful action.
46. As the owner, I want weekly wedding progress reports, so that I can see changes, blockers, and upcoming commitments.
47. As the owner, I want calculations performed deterministically, so that arithmetic does not depend on model guesses.
48. As the owner, I want reports to disclose incomplete or stale coverage, so that I can assess their reliability.
49. As the owner, I want later research to cite trustworthy recent sources, so that I can verify findings.
50. As the owner, I want recommendations distinguished from facts, so that external suggestions do not appear to be confirmed planning records.
51. As the owner, I want research-driven spreadsheet changes previewed and approved, so that new information does not silently alter my plan.
52. As the owner, I want lightweight work routed to an entitled economical model, so that recurring usage remains affordable.
53. As the owner, I want stronger models used only when justified, so that difficult requests remain reliable without routine extra cost.
54. As the owner, I want usage, fallback, and tool error information visible, so that I can evaluate cost and reliability.
55. As the owner, I want new Skills added without giving them every permission, so that the assistant can grow safely.
56. As the owner, I want engineering mentoring available as a later Skill, so that learning support can share approved infrastructure.
57. As the operator, I want secrets excluded from source control, model context, and routine logs, so that integrations do not expose credentials.
58. As the operator, I want a non-root local service with bounded requests and retries, so that failures remain contained.
59. As the operator, I want backups and documented restore procedures, so that a VPS or storage failure is recoverable.
60. As the operator, I want version-pinned compatibility evidence, so that deployment does not rely on assumed SDK APIs.
61. As the maintainer, I want contract-first tools and tests at their observable boundary, so that internals can change without changing user guarantees.
62. As the maintainer, I want separate agents introduced only for a demonstrated isolation need, so that the MVP remains simple.

## Implementation Decisions

### 1. Evidence, terminology, and review gates

- **Secretary Agent:** the single primary conversational coordinator.
- **Skill:** domain instructions and workflow guidance; loading one does not grant authority.
- **Tool:** a bounded, validated operation exposed to the agent.
- **Spreadsheet Adapter:** mapping and validation between supported logical records and actual spreadsheet regions.
- **Operation:** a durable, identifiable attempt to perform a side effect.
- **Preview:** the immutable description of a proposed operation and its preconditions.
- **Approval:** a trusted authorization for one preview, actor, target, and validity period.
- **Reminder:** user-facing schedule intent and metadata linked to a native scheduler job.
- **Delivery:** a channel outcome supported by evidence, separate from job execution.

These definitions are provisional because no existing domain glossary was found. They must remain consistent when later domain documentation is created.

The supplied VPS, Ubuntu version, OpenClaw v2026.10.7 installation, OAuth authentication, working Google access, loopback Gateway, disabled elevated/browser tools, and security-audit result are owner-reported facts. None has been inspected in this session. Phase 0 must record observed versions, configurations, capabilities, and timestamps without exposing secrets.

Workbook mappings, write permissions, record identity, approval transport, audit storage, and runtime compatibility must be resolved before activating the affected production capabilities. Local implementation is now authorized by the user's subsequent instruction. Approval of the testing boundary alone does not authorize deployment. Code review is deferred at the user's explicit request; tests and validation still apply.

### 2. Workbook discovery and data quality assessment

**Current assessment: local XLSX inspection complete.** The companion workbook assessment records all five tabs, 313 nonempty cells, eight formulas, 23 merged ranges, hidden blank rows, data-quality findings, source coordinates, and mapping decisions. All eight formula caches agree with independent evaluation of their current inputs. This does not establish their business meaning or live-engine behavior.

The key findings are shared costs spanning multiple item rows; two unlabeled amounts in the wedding budget; a “SISA BIAYA” formula that sums costs without subtracting payments and omits a row included by the section total; a 100,000 prewedding location-cost discrepancy between tabs; guest estimates mixed with narrative proposals; monthly tasks without a year or exact due dates; and an unlabeled negative household amount. No complete payment ledger, stable record IDs, or explicit wedding preparation statuses were found. Currency is not explicitly declared. Keep affected fields unresolved rather than inventing financial or scheduling facts.

Inspect a read-only copy and retain its checksum. Do not re-save or convert the original during analysis. Inventory visible and hidden sheets, used ranges, header locations and tiers, merged cells, tables, named ranges, validations, protected regions, formulas and cached results, formatting conventions, external references, and cross-tab dependencies. Identify repeated blocks, totals, notes, and display-only regions that are not ordinary records.

For each tab, report its purpose, record granularity, source references, read/write suitability, candidate identifiers, formula regions, and relationships. Distinguish confirmed formula references from inferred business relationships. Do not evaluate untrusted external links or formulas as executable code. Cached formula values may be stale; reading them is not proof of recalculation.

Produce an evidence table for missing required values, inconsistent headers, duplicate or ambiguous records, mixed types, locale-dependent dates or currencies, formula errors, disconnected references, and inconsistent totals. Include source coordinates, observed value or a privacy-safe description, severity, user impact, and an optional proposed correction. Missing, unknown, not applicable, and zero are distinct.

The XLSX is an initial reference artifact. Before live integration, establish which Google spreadsheet is authoritative and compare its structure with the inspected export. An outdated export must not determine live write targets. Do not automatically upload or convert the workbook into a replacement planning system.

### 3. Proposed schema mapping

Use an operator-controlled, versioned registry of explicitly approved spreadsheet IDs. Register stable sheet IDs and validated data regions; do not rely solely on tab names. Each mapping defines header locations, aliases, field types, required values, locale/timezone rules, readable/writable ranges, formula exclusions, identification rules, supported relationships, and read-only or read-write permissions.

The following are candidate logical concepts only. They are not claims about workbook columns or instructions to create new tables.

| Logical concept | Candidate fields | Mapping and identity decision |
| --- | --- | --- |
| Task | title, category, owner, status, priority, due date | Map only observed task regions; require a stable identifier or unambiguous approved key. |
| Vendor | name, service category, contact, notes | A name alone is insufficient when duplicates exist; contacts are sensitive. |
| Budget item | description, category, currency, planned amount, agreed amount | Keep estimates distinct from agreed costs and derived totals. |
| Payment | related item/vendor, amount, currency, payment date, payment kind | A deposit is a payment, not an instruction to replace a total budget. Verify how the workbook represents it. |
| Reminder reference | reminder ID, native job ID, related record, schedule, destination reference, sync state | Native scheduler remains authoritative; destination references must not expose credentials. |
| Source reference | spreadsheet ID, sheet ID, record identity, observed range, retrieval time, mapping version | Row coordinates locate observations; they are not durable identities after sorting or insertion. |

The assessed workbook requires five read views: section-aware budget lines and shared cost groups, guest-group estimates, monthly planning text blocks, column-oriented prewedding options, and household items. The companion assessment defines physical ranges and exceptions. There are no formula links or stable foreign keys between tabs; topical relationships remain candidate links. Workbook-local sheet IDs must not be assumed to match Google tab IDs.

All existing tabs remain read-only for the initial integration. Ambiguous headers, missing identifiers, or schema drift disable affected writes. Record resolution must revalidate identity immediately before a mutation. Introducing stable IDs into legacy regions requires a separate preview and approval. If safe identity cannot be established, keep that region read-only. New task/payment records should use stable IDs from the outset.

### 4. Optional management tabs

The inspection supports adding a task tab and a payment ledger when their write capabilities are introduced. Existing notes do not provide dependable task state, and no dated payment history exists. Preview each proposed tab's structure and any formulas or protections for approval; create none during discovery.

| Proposed tab | When justified | Boundary |
| --- | --- | --- |
| `AI_Index` | Legacy records need explicit links or a searchable index. | Derived references; rebuildable and never sufficient by itself to establish a write target. |
| `AI_Tasks` | Recommended when task writes begin: existing monthly notes lack task IDs, explicit status, and complete dates. | Append individually reviewed task candidates with source references; preserve original notes and unknown dates. |
| `AI_Payments` | Recommended when payment recording begins: existing amounts do not establish a payment ledger. | Stable payment and operation IDs, reviewed item/group link, confirmed amount/currency, explicit payment date and kind; no automatic import of unlabeled amounts. |
| `AI_Reminders` | The owner needs reminder visibility inside the workbook. | Synchronized metadata; editing a row does not directly modify native jobs. |
| `AI_Activity_Log` | The owner wants a readable activity feed. | A projection of operations, never the authoritative audit journal. |

Defer the index, reminder projection, and activity feed until their user-facing value is demonstrated. Do not create all proposed tabs by default. The protected system journal belongs outside the workbook and is inaccessible through ordinary spreadsheet or memory tools.

### 5. Runtime architecture and source-of-truth boundaries

The intended flow is WhatsApp → OpenClaw Gateway → Secretary Agent → authorized Tool → TypeScript Bridge → Rust Core Service → Google Sheets. Skills guide selection and reporting. Reminder operations go through supported native scheduler interfaces; Rust does not implement a scheduler or manipulate native scheduler storage.

| Information | Authority | Secondary representation |
| --- | --- | --- |
| Spreadsheet-managed budgets, payments, vendors, tasks | Approved Google spreadsheet regions | Tool responses and derived reports |
| Explicit personal preferences and conventions | Approved agent-memory facility, selected after inspection | Search results with provenance |
| Native jobs and execution history | OpenClaw native scheduler | Reminder metadata and synchronization records |
| Delivery outcome | Available channel acknowledgments or transport evidence | Reminder delivery history |
| Preview, approval use, idempotency, operation history | Protected durable Core Service storage | User-facing history summaries |
| Research findings | Cited external sources at an identified time | Reports and explicitly approved saved notes |

Retain the proposed Rust/TypeScript split. It adds packaging and a local network boundary, but keeps business rules and provider operations centralized. The bridge remains thin: it must not duplicate mapping, monetary calculations, approval rules, or retry policy.

Use Axum, Tokio, Reqwest, and Serde for the Core Service, with a checked-in OpenAPI contract in the implementation phase. Do not introduce distributed infrastructure. A local SQLite store is the proposed option for operational state, not a replacement for spreadsheet records; architecture review must confirm storage ownership, permissions, backup, and audit retention before adoption.

OpenClaw's current documentation describes tool plugins and a native scheduler, but this does not establish installed-host compatibility. The documented tool-only plugin entry point and its runtime requirements must be checked against the actual installed package. Qualify activation and tool calls on the pinned target runtime; static inspection alone is insufficient. See [Tool plugins](https://docs.openclaw.ai/plugins/tool-plugins) and [Plugin runtime compatibility](https://docs.openclaw.ai/plugins/runtime-compatibility).

The current scheduler documentation uses “Automations” and describes cron as an alias. Preserve native scheduling as the requirement; verify the deployed version's actual interfaces rather than copying current examples into an assumed older API. See [OpenClaw native scheduler](https://docs.openclaw.ai/automation/cron-jobs).

### 6. Tool and API contract

The implementation phase must define bounded typed inputs, outputs, authorization, side effects, limits, pagination, and retry behavior before building endpoints. OpenAPI is the authority for the bridge-to-core HTTP contract. A later executable request collection may derive from it; do not create competing hand-maintained contracts without a synchronization rule.

| Tool | Observable behavior |
| --- | --- |
| `sheets_list` | List only operator-approved registry entries and their supported access modes. |
| `sheets_describe` | Describe approved structure, mapping version, and supported operations. |
| `sheets_read` | Read bounded mapped records with source references and explicit incomplete-coverage information. |
| `sheets_search` | Apply supported typed filters with bounded results and pagination. |
| `sheets_preview_change` | Validate an append or update and return a side-effect-free preview, preconditions, approval requirement, and expiry. |
| `sheets_append` | Commit an authorized preview of an append under its operation identity. |
| `sheets_update` | Commit an authorized preview of an update to an unambiguously resolved record. |
| `sheets_get_history` | Return authorized, bounded operation history; identify its coverage as system-observed changes. |

No arbitrary Google API request, model-selected host URL, filesystem path, credential, or unrestricted mutation range is accepted. Data tools use logical identifiers resolved through the registry. Diagnostic inspection of an unmapped structure remains read-only and operator-scoped.

Google Sheets has no spreadsheet-list endpoint. Registry-based discovery avoids adding broad Drive discovery permissions to the MVP. Broad Drive discovery, if needed later, is a separate permission decision. See [Google's spreadsheet discovery guidance](https://developers.google.com/workspace/sheets/api/guides/create).

Read outputs include spreadsheet/sheet references, observed range or record identity, retrieval time, and mapping version. History is not a fabricated complete history of human edits. Mutation results include an operation ID and precise outcome. Validation failure, denied access, required or expired approval, conflict, rate limit, upstream failure, and uncertain outcome must be distinguishable. Do not mark every error retryable.

Set request/response size, result count, range size, timeouts, and bounded retry limits in reviewed configuration. Exact numeric limits remain an implementation decision based on the actual workbook and VPS, and must be documented before enabling production tools.

### 7. Controlled writes, approval, and recovery

All spreadsheet writes are approval-required initially, including appends. Legacy data, formulas, structural changes, and financial fields always require explicit approval. No standing exemption is introduced by this spec. A record of a payment is not permission to make a financial transaction.

A preview contains the authenticated requester, action, resolved target and identity, exact writable field changes, before/after values, mapping version, observed fingerprint, operation ID, and expiry. Approval arrives through a verified owner interaction, is bound to that immutable preview, and is consumed durably. Model-authored text such as “approved” or a Boolean supplied in tool arguments is not approval evidence. Validate the trusted approval integration in Phase 0; keep writes disabled if it cannot be established.

Persist intent and approval use before a provider mutation. Revalidate mapping, identity, and relevant data under the single-writer coordinator immediately before sending it. Re-read the result and record the verified outcome. Reusing an operation ID with the same payload returns its existing outcome; reusing it with a different payload is rejected. New IDs for semantically repeated requests may still need user clarification; idempotency is not a universal duplicate detector.

Use a durable lifecycle covering prepared, awaiting approval, rejected/expired, authorized, applying, verified success, known failure, conflict, and reconciliation required. A process crash or timeout after a request may mean the write occurred. Such operations stay uncertain until evidence resolves them; do not blindly replay an append. If a workbook cannot support safe reconciliation of uncertain appends without new identifiers or metadata, keep append disabled there until that design is approved.

Single-writer coordination serializes this service's changes only. Fingerprints and a final read detect many stale previews but cannot prevent a human edit between check and write. Do not promise compare-and-swap or exactly-once effects across the local journal and Google. Google documents atomic application of subrequests within one supported spreadsheet batch, while also warning that collaborator changes can affect the resulting document. There is no transaction spanning this service, separate spreadsheets, native scheduler jobs, and WhatsApp. See [Sheets batch update semantics](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets/batchUpdate).

Use minimal updates to mapped input cells. Do not replace entire rows containing formulas, write totals, or clear surrounding formatting. Treat ordinary user text as literal data so a formula-like string is not unintentionally executed. Formula creation or repair is a separate approved operation. Snapshot affected values and relevant formulas/formatting for verification and recovery; undo requires a new preview and conflict check rather than blind restoration.

Record denied or invalid mutation requests safely as well as provider attempts. If the journal cannot durably record an otherwise authorized write, fail closed before sending it. Do not claim disk exhaustion or a process crash can always leave a durable rejection record; surface journal failure through available operational telemetry.

### 8. Reminder lifecycle and WhatsApp delivery

Support create, list, reschedule, pause, resume, cancel, and execution history using version-verified native interfaces. Resolve dates against the request timestamp in Asia/Makassar unless explicitly overridden. Present the absolute date, time, timezone, recurrence, and destination when confirming schedule intent. Clarify ambiguous dates such as an unspecified day “next month”; do not invent a due date.

Persist reminder ID, native job ID, title, resolved schedule, timezone, authorized destination, lifecycle status, last execution, last delivery evidence, synchronization status, and related source record where relevant. Use native state to reconcile missing jobs, metadata failures, and owner edits made directly in the scheduler. Never recreate an intentionally canceled job just because a spreadsheet row is stale. Recurring jobs need per-occurrence execution/delivery history, not only one aggregate Boolean.

Keep queued, executed, send attempted, transport accepted, delivered, failed, and unknown outcomes distinct where evidence supports them. If the connector exposes only send acceptance, report that limitation and do not infer receipt or reading. Version-specific native delivery retries must be understood before adding retry behavior that might duplicate notifications.

Use a dedicated assistant WhatsApp account, explicit owner admission, disabled groups, and an independent outbound destination allowlist. Inbound admission alone is insufficient to constrain outbound recipients. Current OpenClaw documentation specifically distinguishes `allowFrom` from outbound destination control; verify target-version behavior before activation. See [WhatsApp access and delivery](https://docs.openclaw.ai/providers/whatsapp).

Scheduled jobs receive only the tools and data access needed for their purpose. A previously approved report schedule does not authorize new spreadsheet writes, arbitrary recipients, financial transactions, or vendor outreach.

For “Catat DP catering 3 juta, lalu ingatkan aku bayar sisanya bulan depan,” resolve the catering record, currency, deposit representation, and exact reminder time as needed; preview the deposit; commit only after trusted approval; verify it; then create the requested reminder when its schedule is unambiguous. Return the deposit operation outcome and reminder/job outcome separately. If reminder creation fails after a verified deposit write, retain the deposit, report that result, and retry only the unresolved reminder safely.

### 9. Memory, domain Skills, reporting, and models

Use the existing supported agent-memory mechanism where it meets explicit save/update/archive/history requirements. Do not introduce vector storage or a second personal knowledge database by default. Verify permissions, provenance, lifecycle support, and opt-out of unintended saving during discovery; an unavailable history capability becomes an explicit gap rather than an invented feature.

Structured external memory remains in its approved authoritative system. Retrieved spreadsheet or web text is untrusted data and cannot authorize tools, save secrets, change policy, or supply an approval. Persist a sensitive personal fact only following an explicit save request with clear scope.

Personal Secretary coordinates general tasks and memory. Wedding Planner uses the mapped existing workbook for vendors, tasks, budgets, payments, and priorities. Report Assistant produces source-linked reports. Research Assistant is a later extension with approved source access and citations. Engineering Coach is a later teaching Skill. Enforce permissions through runtime and service policy; Skill text alone is not a security boundary. Where one shared agent cannot enforce domain-specific capability isolation, withhold the sensitive capability or review a stronger isolation design.

Compute financial values in Rust using exact decimal or integer minor-unit arithmetic and explicit currency/rounding rules. Derive outstanding balances only when the source semantics support that calculation. Do not sum mixed currencies without an approved conversion policy. Reports state retrieval time, source coverage, relevant missing fields, assumptions, and independently labeled recommendations. Reading multiple tabs is not automatically a consistent transactional snapshot.

Model choice is capability- and entitlement-driven. “Luna” is a preference, not a verified identifier or entitlement. Inspect the configured OpenClaw model registry before selecting exact models. Do not infer API access or pricing from ChatGPT/Codex OAuth. Use deterministic code and native fixed reminders to reduce model calls. Establish bounded escalation and fallback rules, logging token usage when available, tool counts, errors, fallback frequency, and applicable estimated API cost. Do not invent currency costs for unpriced subscription usage.

### 10. Security, audit, and operations

Bind the Core Service to loopback, authenticate all business requests, derive actor identity from trusted invocation context, and enforce the registry and permissions inside Rust. Localhost does not imply authorization. Do not accept a caller-selected actor or approval identity as authoritative. Restrict the bridge's backend destination and keep its secret out of model-visible results.

Run as a dedicated non-root systemd service with limited filesystem access. Keep the existing service-account credential outside Git and outside the agent-accessible workspace. Restrict file permissions and approved spreadsheet sharing; avoid broad Google Cloud IAM grants. Verify scopes and file access independently. Google notes that Sheets scopes apply to an entire spreadsheet, not individual tabs; enforce tab/field policy in the adapter and inspect protections as an additional safeguard. See [Sheets authorization scopes](https://developers.google.com/workspace/sheets/api/scopes).

Maintain operation ID, actor, timestamps, target, approval reference, requested change, provider attempt/outcome, and reconciliation evidence in protected operational storage. Store only necessary values, with defined retention and restricted access. User-facing logs contain safe summaries. The agent has no journal rewrite/delete tool. A service-owned journal is not tamper-proof against compromise of that service or the VPS administrator; stronger independent audit retention can be reviewed later.

Specify backups for operational state, relevant configuration, native scheduler persistence, and recovery evidence. Keep secret backups separate and protected. Verify restoration on a disposable environment. Startup reconciliation must inspect interrupted operations without replaying uncertain side effects. Dependency lockfiles, formatting, linting, tests, secret scanning, security review, deployment health checks, and documented rollback belong to each applicable implementation phase. Do not prescribe commands whose existence has not been verified against the selected tools.

### 11. Risks and trade-offs

| Risk or trade-off | Decision or mitigation | Remaining limitation |
| --- | --- | --- |
| Export differs from authoritative live source | Local artifact assessed; compare its mapping with live metadata before registration. | No live-source or runtime compatibility proof yet. |
| Irregular legacy layouts | Map bounded regions and retain unsupported areas as read-only. | Some workflows may require separately approved structural additions. |
| Rust plus TypeScript | Centralize rules in Rust and keep the bridge thin. | Two build/deployment artifacts and an authenticated boundary remain. |
| Human concurrent edits | Serialize service writes, re-resolve identity, compare fingerprints, verify results. | The read/write race cannot be eliminated by this design. |
| Write succeeds before timeout/crash | Durable intent and reconciliation; suspend uncertain retries. | Exactly-once effects are not guaranteed. |
| Google quotas, outages, credential failure | Bound retries, classify errors, expose incomplete results. | Availability depends on external services. |
| Approval replay or prompt injection | Bind trusted approval to the immutable preview and enforce service policy. | Runtime approval integration must be qualified. |
| Audit co-located with the service | Separate storage from agent tools, restrict access, back up. | Service/host compromise can still affect it. |
| Scheduler succeeds but delivery fails | Track execution and channel evidence separately. | Connector may not expose delivery receipts. |
| Per-Skill permissions in a shared agent | Enforce actual runtime/service capabilities. | Instructions cannot create isolation that the runtime lacks. |
| Model identifier, entitlement, or SDK changes | Inspect and pin actual versions and available models. | Documentation review alone cannot prove deployment readiness. |
| Reports span changing sources | Include time and coverage; recheck consequential inputs. | No automatic cross-source consistent snapshot. |

### 12. Incremental implementation plan

Each phase requires observable acceptance criteria, relevant automated tests, failure handling, logs, dependency/security review, documentation, and a rollback or recovery procedure. The sequence preserves the master plan; production mutations remain disabled until the write phase passes its gates.

| Phase | Scope | Acceptance gate and recovery expectation |
| --- | --- | --- |
| 0 — Discovery & Architecture | Inspect workbook; map relationships and quality; verify runtime, service-account access, approval/delivery capabilities; record decisions and backlog. | Complete the seven requested assessment outputs with evidence; owner reviews architecture before implementation. Discovery must leave source files and live sheets unchanged. |
| 1 — Repository & Foundation | Establish private repository, Rust/TypeScript packages, OpenAPI, configuration examples, CI, and operator documentation after review. | Builds and checks pass without production credentials; secrets stay excluded; a documented clean setup works. No production deployment implied. |
| 2 — Read-only Sheets | Authentication, registry, mappings, describe/read/search tools and source references. | Real supported fixture structures are readable; unknown mappings fail safely; provider write requests are impossible in this mode; access denial and pagination are covered. Disable plugin to roll back. |
| 3 — Controlled Writes | Preview/approval, append/update, operation journal, idempotency, conflict handling, verification, reconciliation. | Approval required; expired/tampered requests rejected; retries do not replay known operations; uncertain writes reconcile; formulas/formatting preserved on a disposable sheet. Disable writes and use reviewed recovery operations if needed. |
| 4 — Secretary & Memory | Personal Secretary Skill, explicit memory lifecycle, intent/tool guidance, escalation policy. | Source facts override stale conversation; sensitive saves require explicit intent; memory lifecycle and permissions verified. Disable memory mutation independently if needed. |
| 5 — Native Reminders | Create/list/reschedule/pause/resume/cancel, timezone handling, metadata synchronization, restart behavior. | Jobs persist; ambiguous dates are clarified; cancellation is respected; execution is not delivery. Test without WhatsApp; pause test jobs for rollback. |
| 6 — WhatsApp | Dedicated account, owner admission, groups disabled, conversational writes and reminders, outbound evidence. | Unauthorized senders and destinations are rejected; approved round trips work; failure/unknown delivery stays explicit. Disable the channel and pause outbound jobs if needed. |
| 7 — Wedding Planner | Existing-workbook task/vendor/payment workflows and weekly progress reporting. | Source-linked answers and deterministic totals match inspected data; the deposit-plus-reminder example reports partial outcomes truthfully. Disable the Skill without changing workbook data. |
| 8 — Research & Reporting | Approved research, Report/Research Skills, scheduled reports, usage visibility. | Factual claims cite sources; recommendations are separate; reports use current records and disclose gaps. Disable schedules/research tools independently. |
| 9 — Optional Agent Expansion | Evaluate independent context, models, permissions, long-running work, parallelism, or memory ownership. | Separate proposal demonstrates need, isolation, delegation limits, and budgets; no automatic promotion from Skill to agent. Disable added agents independently. |

The MVP completes Phases 0–7: safe retrieval and approved changes, traceability, persistent reminders, WhatsApp delivery evidence, and a Wedding Planner using the actual existing workbook. Research/report extensions and multiple agents do not block the MVP; basic wedding reports are included in Phase 7.

## Testing Decisions

The owner confirmed the primary seam: test observable tool-to-core behavior at the agent-facing tool interface, adding adapter-level tests only for spreadsheet preservation, conflicts, retries, and delivery evidence. No existing test prior art is present in this workspace.

1. Prefer deterministic integration tests that invoke the real TypeScript bridge and Rust service through their public contract. Assert results, authorization, persisted state, and externally visible provider effects; do not assert private helper calls or internal class structure.
2. Use sanitized fixtures derived from the inspected workbook, preserving difficult layouts, formulas, merged headers, duplicates, locale values, and missing fields. Include the three shared cost groups, the excluded-row subtotal dependency, narrative guest counts, yearless multiline tasks, text option totals, and formatted empty household cells. Do not commit the personal workbook or its sensitive values by default.
3. Exercise the real service against provider stand-ins for timeouts, throttling, authorization errors, malformed responses, schema drift, read/write races, and a write accepted before its response is lost. Validate restart recovery and operation-ID reuse at the same observable boundary.
4. Add focused adapter tests where the top-level response cannot establish preservation or ambiguity behavior. Check source cells, formulas, and relevant formatting before and after approved changes, including external row sorting/insertion.
5. Cover missing, rejected, expired, replayed, and altered approvals; forbidden spreadsheets/fields; caller-forged identities; read-only mode; formula-like literal input; journal unavailability; and hostile instructions embedded in cells or research data.
6. Assert idempotency for same-ID/same-payload requests, rejection of same-ID/different-payload requests, and explicit uncertainty after ambiguous provider results. Demonstrate that reconciliation cannot silently produce a duplicate append.
7. Test scheduler workflows through the verified supported interface. Use controlled clocks where possible to verify timezone resolution, recurrences, pause/resume/cancel, restart persistence, and missing metadata. Keep job execution distinct from channel send and receipt evidence.
8. Test composed operations with deposit success/reminder failure, deposit failure, uncertain deposit outcome, metadata failure after job creation, and recovery. The user summary must match each actual effect.
9. Test deterministic reports against known results, including unknown balances, missing amounts, currencies, rounding, incomplete pagination, and stale source coverage. Do not use exact model wording as a proxy for financial correctness.
10. Run explicit end-to-end acceptance checks on the pinned OpenClaw/runtime versions and a disposable Google spreadsheet before enabling production writes. Mock success is not evidence of Google compatibility; documentation is not a plugin activation test.
11. Test WhatsApp only after owner-reviewed configuration, using the dedicated account and authorized destination. Record the strongest delivery evidence actually available and verify that absence of a receipt remains unknown.
12. During Phase 0, perform documentary and read-only evidence checks only: complete inventory, traceable findings, correct source boundaries, and explicit unresolved assumptions. No application tests have been run for this spec because no application exists.

## Out of Scope

- Activating production capabilities before their documented qualification gates pass.
- Fabricated workbook findings, guessed live mappings, or unverified SDK commands and model IDs.
- Replacing Google Sheets with a custom record database or importing the XLSX as a new planning system automatically.
- Automatic cleanup, renaming, restructuring, deletion, formula repair, or creation of proposed management tabs.
- A custom scheduler, WhatsApp gateway, dashboard, Kubernetes, Redis, message broker, or distributed deployment.
- Unrestricted shell, elevated tools, arbitrary filesystem/browser access, arbitrary Google API requests, or broad Drive discovery by default.
- Payments, purchases, vendor outreach, group messaging, or arbitrary outbound recipients.
- A separate calendar-provider integration; schedules in this MVP are reminders and source-linked planning records.
- Transactional guarantees across Google Sheets, local state, scheduler jobs, and messages; native optimistic locking or exactly-once delivery promises.
- A complete history of human spreadsheet edits or audit protection against full VPS compromise.
- Immediate multi-agent orchestration, vector storage, or production Postman workspace setup/publication.

## Further Notes

### Phase 0 assessment status

| Requested output | Current status | Required evidence |
| --- | --- | --- |
| Existing structure and relationships | Complete for local XLSX: five tabs, merges, sections, formula lineage, and candidate topical links documented. | Compare with the authoritative live spreadsheet before production mapping. |
| Data quality findings | Fifteen findings with source coordinates, automation impact, and proposed treatment. | Unresolved financial meanings and schedule details remain explicit. |
| Proposed schema mapping | Physical read views and exceptions documented; legacy writes disabled initially. | Approve record identity and confirm live structure before enabling affected operations. |
| Recommended new tabs | Task and payment tabs recommended for later write phases; other projections deferred. None created. | Review exact schema and tab-creation previews. |
| Architecture and security decisions | Draft updated from actual workbook evidence. | Actual runtime, authorization, credential, and persistence inspection remains pending. |
| Risks and trade-offs | Updated for merged costs, incomplete amounts, missing dates/IDs, and export/live differences. | Live/runtime evidence may refine mitigations. |
| Incremental implementation plan | Revised around read-only legacy views followed by approved structured additions. | Remaining Phase 0 checks and owner architecture review precede implementation. |

### Evidence recorded in this session

- The workspace was initially empty. It now contains this specification, a companion assessment, and the owner-supplied workbook. It is not a Git repository; no applicable project instruction file, glossary, ADR, API contract, or application test suite was found.
- The owner supplied the local workbook after the initial draft. All five tabs were inspected using read-only ZIP/XML analysis, all eight current formula caches were independently reconciled, ZIP integrity was checked, and the source checksum remained unchanged. Live Google access verification has not occurred.
- A read-only GitHub lookup could not resolve `gdochadipa/openclaw-secretary`. This does not establish that the intended repository is absent under another owner. The owner then explicitly requested local-only delivery, so no repository, issue, label, or tracker configuration was created.
- The owner explicitly accepted the proposed testing boundaries. This specification is still awaiting architecture review.
- Current official Google Sheets and OpenClaw documentation was consulted; references appear beside the supported claims. This is documentation evidence only, not target-host compatibility evidence.
- Postman's API engineering, discovery, and documentation guidance informed the contract decisions. The Orbit discovery request failed at DNS resolution, so no discovery brief or provider compatibility result was obtained through it. Official provider documentation supplied the referenced facts instead. No Postman workspace was initialized or published.
- No application code, Google records, scheduler jobs, channel configuration, credentials, or production resources were modified. No application tests or deployment checks were run.

### Companion assessment and remaining discovery

See the [workbook assessment](../spreadsheet-assessment.md) for the actual structure, quality findings, physical mapping proposal, recommended additions, security decisions, and implementation sequence.

Identify the authoritative live Google spreadsheet and compare it with the assessed export. Confirm relevant field meanings, currency, record identity, and scheduling details before enabling dependent operations. Supply only safe access context; do not paste the service-account private key into conversation. VPS inspection must use an available authorized access method; none was established in this session.

Complete the assessment before asking for architecture sign-off. After sign-off, each implementation phase can be specified and delivered incrementally. If tracker publication is requested later, publish to a confirmed private repository and scope any `ready-for-agent` designation to the authorized discovery work; it must not override the implementation review gate.
