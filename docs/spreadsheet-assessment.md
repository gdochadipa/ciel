# Persiapan Nikah — Workbook Assessment & Integration Proposal

Date: 2026-10-25  
Status: Local workbook assessment complete; live-source and runtime verification pending; architecture review required  
Scope: Read-only inspection of the supplied XLSX. No workbook edits, formula repairs, tab creation, or application implementation.

## Recommendation

Keep all five existing tabs and their layouts. Start with a read-only adapter that understands sections, merged cost groups, narrative guest estimates, task text blocks, and comparison columns. Do not expose a generic “one row equals one record” writer against this workbook.

The first useful additions, after architecture review and write-control implementation, are a structured task tab and a payment ledger. Neither exists in a form suitable for dependable task scheduling or payment history. Keep legacy budget updates disabled until field meanings, record identity, and bundled costs have been reviewed. New tabs remain proposals and have not been created.

The workbook contains no trustworthy basis for calculating a complete outstanding wedding balance. Its headline cost total is 144,800,000, but several costs are unspecified and the row labeled “SISA BIAYA” does not subtract payments. Currency is not explicitly declared in the inspected headers or number formats. Treat IDR as a contextual hypothesis until confirmed, and do not label these amounts as settled costs or cash already paid.

## 1. Existing structure and relationships

### File evidence

Source: `docs/Persiapan Nikah.xlsx`  
Size: 26,608 bytes  
SHA-256: `b24351dbda4c69a7cbf5f14d9b18d7fbbc983deff406e8dad3607f6bcd196de3`

The XLSX package contains five visible worksheets, 313 nonempty cells, eight formulas, and 23 merged ranges. No hidden sheets, defined names, Excel Tables, filters, validation rules, conditional-format rules, worksheet protection, frozen panes, hyperlinks, external workbook-link parts, or chart objects were found. Five drawing parts exist but contain no drawing objects. These observations describe this export, not necessarily the live Google spreadsheet or features that may not survive export.

All eight formulas use same-sheet `SUM` references. There are no formula links between tabs. No stored Excel error-type cells were found. Cached values for all eight formulas match an independent evaluation of their current inputs, including the spreadsheet behavior of ignoring text and blank cells. This is a static reconciliation, not proof of recalculation in Excel or Google Sheets and not proof that the formulas have the intended business meaning.

| Existing tab | Nonempty envelope | Populated cells / formulas / merges | Actual role |
| --- | --- | --- | --- |
| `Sheet1` | A1:G63 | 194 / 7 / 20 | Wedding cost plan grouped into five event sections; item, vendor, cost, funding source, and an unused preparation column. |
| `Sheet2` | A1:F13 | 46 / 1 / 0 | Two guest-estimate blocks, with counts, names, commentary, and attendance estimates mixed together. |
| `Sheet3` | A3:C26 | 25 / 0 / 3 | Monthly planning notes split between two people; multiple tasks stored in individual multiline cells. |
| `Sheet4` | A1:C10 | 24 / 0 / 0 | Two prewedding options arranged as columns, with attributes as rows. |
| `RUMAH` | A1:D16 | 24 / 0 / 0 | Fourteen household purchase labels; three prices and three explicit `YA` settlement flags. |

“Nonempty envelope” describes cells containing a value or formula. It is not a provider-reported used range. In particular, `RUMAH` contains 2,016 stored cell elements but only 24 nonempty cells. Avoid turning formatted empty cells into thousands of records.

### Wedding budget (`Sheet1`)

Headers are B1 `KEPERLUAN`, C1 `VENDOR`, D1 `BIAYA`, F1 `SUMBER BIAYA`, and G1 `PERSIAPAN`. A contains section labels. E has no header but contains amounts in E4 and E7. G has a header but no populated status values beneath it.

| Section | Actual item rows | Numeric D cells | Text `-` cells | Cost subtotal and formula |
| --- | --- | --- | --- | --- |
| KEPERLUAN AWAL | 2–8 | 5 | D5, D8 | D9: `SUM(D2:D8)` = 8,750,000 |
| PRE WEDDING | 12–17 | 4 | D14 | D18: `SUM(D12:D17)` = 9,000,000 |
| MESEDEK | 20–26 | 4 | D22, D24, D26 | D27: `SUM(D20:D26)` = 1,200,000 |
| NGIDIH | 29–44 | 14 | D35, D41 | D48: `SUM(D29:D44)` = 29,250,000 |
| HARI - H | 50–59 | 6 | D56 | D60: `SUM(D50:D59)` = 96,600,000 |

There are 46 item positions: 33 numeric cost anchors, nine text dashes, and four blank D cells belonging to merged cost groups. Forty-five positions have a B-column item label; row 17 has its service description in C17 instead. A value of `-` does not establish zero: nearby notes variously describe sponsorship, personal ownership, reuse, or an unspecified cost.

Preserve these bundled relationships explicitly:

- D12:D13 holds one 6,200,000 package cost across makeup and photography. Count the cost once. The two rows have different vendor cells.
- D50:D52 holds one 82,000,000 cost across the venue/event line and two additional labels. The merge is evidence of a shared cost region, not proof that every subservice is contractually included.
- C53:C54 and D53:D54 share vendor and 7,000,000 cost for two makeup labels.
- C20:C21 and C29:C30 share vendors across clothing rows. These are vendor groups, not shared prices: each of their D cells contains a separate amount.
- F50:F54 and F56:F58 share funding-source labels. Propagate them only inside the actual merge.

The section-label merges do not cover every item: A1:A7 excludes row 8, and A12:A16 excludes row 17. Use explicit reviewed section boundaries rather than generic fill-down from A. Rows 45–47 are hidden and contain no values or formulas; exclude them from records without deleting or unhiding them.

D10, labeled “SISA BIAYA,” is `SUM(D2:D7)` = 8,750,000. B63, labeled “Total Keseluruhan,” is `SUM(D60,D48,D27,D18,D10)` = 144,800,000. The grand total depends on D10 rather than D9. Their values currently agree only because D8 contains text. If D8 later becomes numeric, D9 changes while D10 and B63 do not include it.

### Guest estimates (`Sheet2`)

- Left block: A2:A10 contains group labels where populated; B2:B10 combines a declared count with guest names and notes. B12 is a manually entered total of 126. The leading counts in B2:B10 also sum to 126. Some labels are blank and some comments mention partners, so do not count names as individual confirmed attendees.
- Right block: C2:C10 contains group labels, D3:D7 contains five numeric group counts, and E/F contain narrative comments. D12 is `SUM(D3:D10)` = 74. A proposed family count of 70 appears in F2, not in a numeric count cell, and is not included by that formula. Do not silently change the total to 144 or declare that proposal accepted.
- B13, D13, and E12 contain additional attendance estimates, including a range and a comment. They are not RSVP outcomes or reliable replacements for the nominal group counts.

These blocks describe estimates at different levels and cannot safely be combined into a confirmed guest total or used to revise catering quantities automatically.

### Planning notes (`Sheet3`)

A4:C4 labels month and two people. October occupies A5:A10, B5:B10, and C5:C10 as merged blocks. Later month labels occupy rows 11–14 and 16–19. There is no year, exact deadline, stable task ID, status field, or reminder reference. The sequence spans October through July and omits some month labels. A26:C26 is a separate miscellaneous planning region with no established wedding relationship.

Treat each multiline cell as a source text block. Proposed task extraction must retain the source cell and original wording, separate each proposed action for review, and leave exact dates and completion status unknown. A month name alone cannot create a dated reminder. Do not turn isolated hyphens or blank bullet entries into tasks.

### Prewedding comparison (`Sheet4`)

B3 and C3 name the two options. A4:A10 contains price, locations, makeup, edited photos, printed photos, additional costs, and total. B4 and C4 are numeric base prices. Additional costs are multiline text, and B10/C10 totals are text strings using `k`/`K`, not formulas.

Interpreting the explicitly written `k` values as thousands, option B's 6,200,000 + 700,000 + 500,000 + 1,500,000 = 8,900,000 matches B10. Option C's 6,800,000 + 1,100,000 + 500,000 + 1,500,000 = 9,900,000 matches C10. Retain that interpretation as a parsed projection with its original text, not a rewrite of the cells.

The first option appears related to the PRE WEDDING section, but there is no explicit selected-option flag or formula link. `Sheet1!D15` contains 800,000 for the location, while `Sheet4!B9` describes 700,000 for the corresponding location. This accounts for a 100,000 difference between the 9,000,000 budget subtotal and the 8,900,000 comparison total. The workbook does not establish which quote is current.

### Household purchases (`RUMAH`)

A3:A16 contains fourteen purchase labels. B3:B5 contains three prices totaling 9,193,000, and C3:C5 contains `YA` under `LUNAS`. B6:B16 and C6:C16 are blank: prices and settlement state are unknown. D5 contains an unlabeled negative 1,700,000. Do not assume it is a discount, payment, contribution, or correction, and do not net it against the listed prices.

Keep household planning outside the wedding budget total unless the owner explicitly defines a reporting boundary that combines them.

### Relationship map

| Relationship | Evidence | Confidence and permitted use |
| --- | --- | --- |
| Section inputs → budget subtotals → grand total | Seven same-sheet formulas in `Sheet1`. | Formula links are confirmed; the “remaining cost” semantics are unresolved. |
| Makeup/photography and event packages | Actual merged cost/vendor/funding regions described above. | Shared storage is confirmed; service inclusion must not be invented. |
| Prewedding comparison → wedding budget | Matching service descriptions and base price, with a location-cost discrepancy. | Candidate business relationship only; retain separate sources and flag the discrepancy. |
| Monthly planning → vendors/budget items | Similar topics recur in free-text task notes. | Candidate links requiring reviewed mapping; no stable foreign keys. |
| Guest estimates → catering/event capacity | Counts and capacity descriptions exist on separate tabs. | Useful comparison context only; event attendance scope and confirmed counts are missing. |
| Household purchases → wedding budget | No formulas or explicit identifiers link them. | Separate domains by default. |

## 2. Data quality findings

Severity here describes the impact on safe automation. It does not assert that the owner's informal planning style is wrong.

| ID | Severity | Source evidence | Impact and recommended treatment |
| --- | --- | --- | --- |
| Q01 | High | `Sheet1!E4`, E7 contain 2,000,000 and 750,000 under an unlabeled column. | Cannot classify as deposits or contributions. Preserve as unmapped amounts; confirm meaning before any payment calculation. |
| Q02 | High | `Sheet1!D10`, D9, D8, B63. | “SISA BIAYA” is a sum with a different range, and the grand total uses it. Do not report an outstanding balance; propose formula correction separately after semantics are reviewed. |
| Q03 | High | Shared costs in D12:D13, D50:D52, D53:D54. | Naive fill-down duplicates package costs. Model cost groups and exclude merged children from independent amounts. |
| Q04 | High | Nine dashes in budget cost ranges; household price blanks. | Complete budget and remaining balance are unavailable. Preserve unknown/sponsored/reused distinctions instead of coercing all to zero. |
| Q05 | High | `Sheet3!A5:C19`. | No year or exact due date. Do not create reminders until schedule intent is confirmed. |
| Q06 | High | No stable record IDs across the tabs. | Row coordinates are insufficient after sorting/insertion. Keep legacy writes disabled until identity and revalidation are approved. |
| Q07 | Medium | `Sheet1!D15` vs `Sheet4!B9:B10`. | Prewedding location amount differs by 100,000. Show both source values; ask which is current when a consequential calculation or edit depends on it. |
| Q08 | Medium | `Sheet2!D12` vs F2 and D13. | Numeric total excludes a proposed family estimate; other notes describe different attendance assumptions. Report by block/scenario, not a single confirmed headcount. |
| Q09 | Medium | `Sheet4!B9:C10`; text counts in `Sheet2!B2:B10`. | Amounts/counts are embedded in text. Use reversible parsing with source text and explicit units; reject ambiguous conversions. |
| Q10 | Medium | `RUMAH!D5`. | Unlabeled negative amount cannot be included in cost or payment arithmetic. Preserve separately until clarified. |
| Q11 | Medium | `Sheet1!C4`, C6:C7, C32, C34:C38, C43:C44, C57:C59 include placeholders or alternatives. | A vendor cell may be a shortlist or note, not a booked supplier. Do not create a confirmed vendor or send outreach from it. |
| Q12 | Medium | `Sheet1!G2:G59` is blank; `Sheet3` has no status field. | Preparation completion percentages cannot be computed from explicit states. Color is not a substitute for status. |
| Q13 | Medium | `Sheet1!A1:A7`, A12:A16, C17; merged labels and hidden rows. | Generic section detection would omit or misclassify items. Use explicit ranges and preserve hidden rows. |
| Q14 | Medium | Cost headers and numeric formats do not declare a currency. | Confirm currency before financial writes or cross-source aggregation; do not infer model output units silently. |
| Q15 | Low | Generic tab names; mixed spelling/case, trailing whitespace; color fills without a legend. | Add descriptive aliases in the registry and normalized search projections. Preserve original names, wording, and colors. |

No confirmed formula arithmetic error was found for current numeric inputs. Q02 is a range/meaning problem whose financial impact becomes visible when the omitted cost changes. No corrective edits have been applied.

## 3. Proposed schema mapping

### Read-only mappings for existing tabs

| Logical record/view | Source mapping | Normalization and write policy |
| --- | --- | --- |
| Wedding cost line | Five explicit `Sheet1` row blocks above; B description, C vendor text, D cost anchor, F funding source. | Return original text, source cell, section, bundle ID, observed numeric amount or unresolved cost state. Row 17 uses C17 as a description exception. Legacy write disabled initially. |
| Shared cost group | D12:D13, D50:D52, D53:D54. | One amount per anchor with references to all covered lines; no duplicated allocation. Amount allocation requires a separate owner decision. |
| Vendor candidate/group | C cells and C20:C21, C29:C30, C50:C52, C53:C54. | Keep alternatives and placeholders as raw vendor notes; group inheritance only within merges. Distinguish vendor candidate from selected vendor. |
| Funding source | F cells, especially F50:F54 and F56:F58. | Owner labels are not proof that a contribution was paid. Do not infer percentage splits. |
| Budget summary | D9, D10, D18, D27, D48, D60, B63. | Derived source outputs, always read-only; return formula lineage and incomplete-cost caveats. |
| Guest group estimate | Left A2:B10 and right C2:F10, with summaries/notes separate. | Preserve family side, group label, raw count text, parsed declared count where unambiguous, and notes. No individual attendee records or RSVP status inferred. |
| Planning text block | `Sheet3` month anchors and B/C source cells; row 26 separate. | Return month label without inventing year, nominal owner column, raw text, and proposed task candidates. Exclude empty bullets. |
| Prewedding option | `Sheet4!B3:B10` and C3:C10, with A4:A10 as attributes. | Column-oriented records; numeric base amount and raw additional-cost/total strings. Selection and quote date remain unknown. |
| Household item | `RUMAH!A3:C16`. | Numeric price if present; `YA` maps to explicitly recorded settled state, blank to unknown. D5 is an unmapped annotation. |

Add source revision/fingerprint, mapping version, and retrieval time to every response. Locators in this assessment are discovery evidence, not durable record identities. XLSX sheet IDs 1–5 are local package IDs; discover and register actual Google spreadsheet/tab IDs rather than assuming they match.

The registry should give the generic tabs descriptive aliases without renaming them, declare exact record regions, and exclude headers, summary formulas, hidden blank rows, and styled empty tails. Detect drift in headers, merge topology, section ranges, and formula locations. Suspend affected operations when the live structure differs.

For future legacy updates, require unambiguous reviewed identity and current target validation. A sidecar index alone does not establish identity after a reorder. Introduce stable IDs only through an approved structural change, or keep those legacy regions read-only.

### Candidate structured additions

| Concept | Required fields and behavior |
| --- | --- |
| Task | Stable task ID, title, optional owner, explicit status, optional priority, optional absolute due date/time with timezone, original source reference, created/updated metadata. Preserve unknown year/date/status. Source prose stays unchanged when a task is approved. |
| Payment | Stable payment ID, operation ID for safe retry/reconciliation, approved cost-group/item link, exact amount and confirmed currency, payment date, payment kind, optional note, source reference. Require explicit authorization; do not infer payment from an unlabeled number. |
| Reminder reference | Reminder ID, native job ID, related task/payment reference, schedule/timezone, approved destination reference, execution/delivery evidence, synchronization state. Store operationally by default; optional worksheet projection later. |

The payment date must come from the user's statement or confirmation, not silently from the tool execution timestamp. A future payment ledger records transactions already made; it does not perform financial transfers. Avoid changing the legacy budget formulas to depend on new tabs until a separate reviewed formula proposal establishes the intended balance calculation.

## 4. Recommended new tabs

1. **`AI_Tasks`: recommended when structured task writes begin.** The monthly prose lacks reliable task identity, status, and dates. Present extracted task candidates for review and append only approved tasks. Do not automatically mark the original notes as completed or migrate them away.
2. **`AI_Payments`: recommended when payment recording begins.** The existing tabs do not provide dated payment history. Link approved payment records to reviewed cost groups; resolve the unlabeled E-column amounts and currency before importing historical entries. This addition is beyond the master plan's example names and addresses an observed gap.
3. **`AI_Index`: defer.** The initial adapter can keep mappings in protected service configuration. Add a user-visible index only if navigation and cross-links need it; never trust editable index rows as authorization.
4. **`AI_Reminders`: defer until scheduling is active and worksheet visibility is useful.** Native scheduler state remains authoritative. An edited metadata row cannot create, reschedule, or cancel a job by itself.
5. **`AI_Activity_Log`: optional later.** A readable projection may help the owner; the real audit journal stays outside the workbook and cannot be rewritten through agent data tools.

Do not add a duplicate vendor database, guest registry, dashboard, or normalized replacement budget by default. If the owner later needs individual RSVPs or vendor booking history, assess those requirements separately.

## 5. Architecture and security decisions

- Retain one Secretary Agent, bounded Tools, domain Skills, a thin TypeScript bridge, and the Rust Core Service. This assessment adds workbook-specific mapping decisions, not application code or a new API surface.
- Keep all existing tabs read-only in the first integration. Search and reporting can work immediately after registry/authentication implementation. Writes first target approved structured additions with stable IDs and operation metadata; legacy edits need additional reviewed identity and merge-aware rules.
- Preserve Google Sheets as authority for planning records once the live source is identified. This local XLSX is evidence of one snapshot. Reinspect the live tab structures before accepting its mapping for production.
- Do not use private planning data as tool instructions. Task notes, vendor notes, cell colors, and spreadsheet text cannot approve writes, grant tools, set recipients, or persist sensitive memory.
- Restrict reads by domain and purpose: guest names, personal planning notes, and household records should not be copied into unrelated reports or long-term agent memory. Keep fixtures sanitized. This assessment intentionally avoids reproducing full guest lists or personal task text.
- Apply operator-controlled spreadsheet/tab/field allowlists and write policy in Rust. A model-supplied range, source link, approval flag, or edited `AI_Index` row cannot bypass policy.
- Keep durable previews, approvals, operation IDs, reconciliation evidence, and audit records in protected service storage. User-editable activity/reminder tabs are projections, not authorization or execution authorities.
- Use native scheduling only after exact dates and destination are confirmed. A task's October/November label does not establish a year or a scheduled job.
- Retain original formulas, formatting, merges, row visibility, and names. Static green fills have no documented semantic legend; do not infer completion or payment state from them.

## 6. Risks and trade-offs

| Choice or risk | Benefit / handling | Cost or residual limitation |
| --- | --- | --- |
| Preserve the legacy presentation | Existing planning remains familiar and intact. | The adapter needs explicit section and merge rules. |
| Keep legacy writes disabled initially | Prevents ambiguous identity and bundled-price updates. | Early automation is strongest for search, summaries, and newly approved structured records. |
| Add only tasks and payments when needed | Addresses observed missing capabilities without replacing the budget. | New records require explicit cross-links and consistency rules. |
| Leave unmapped values unresolved | Avoids inventing deposits, currency, discounts, or paid balances. | Some financial questions must return “unknown” until clarified. |
| Parse text counts and `k` amounts into views | Makes useful data searchable while preserving source text. | Parsing remains a proposal where semantics are unclear; it does not prove commitment or attendance. |
| Use stable IDs for new records | Supports safe retries and references. | Human edits, deletions, and copied IDs still require conflict/reconciliation handling. |
| Inspect export before live integration | Establishes concrete fixtures and mapping requirements. | Exported IDs/features and cached values cannot prove Google runtime behavior. |

## 7. Incremental plan and acceptance criteria

1. **Complete the remaining Phase 0 evidence.** Identify the authoritative live spreadsheet, compare its headers/merges/formulas with this snapshot, and verify actual OpenClaw/Plugin SDK, service-account access, approval transport, native scheduler, and delivery evidence. Record gaps without exposing credentials. Review this assessment together with the master specification before implementation.
2. **Build the foundation after review.** Define registry configuration and bounded tool contracts, sanitized structural fixtures, authentication, CI, and a read-only service mode. Accept when a clean local build/check succeeds without production secrets.
3. **Implement read-only views.** Support the five tab shapes, shared costs, reversible parsing, source references, and explicit unknowns. Accept when all eight formula inputs/results reconcile, 46 budget item positions retain their group semantics, `RUMAH` yields fourteen items rather than 2,016, and no provider write can be issued.
4. **Implement controlled structured writes.** Review `AI_Tasks`/`AI_Payments` schemas and tab-creation previews, then implement trusted approvals, durable journal, stable IDs, idempotency, verified persistence, and uncertain-result reconciliation on disposable sheets. Accept when retries do not duplicate a payment and legacy tabs remain unchanged.
5. **Enable approved task extraction and native reminders.** Review individual task candidates without altering source prose; require exact schedule details before creating jobs. Accept when month-only notes cannot silently schedule, jobs survive restart, and execution/delivery states remain separate.
6. **Add WhatsApp and Wedding Planner workflows.** Enforce owner/destination allowlists and disabled groups. Test the deposit-plus-reminder workflow against an unambiguous item/group, returning separate outcomes for payment recording and reminder creation. Do not assume the workbook's two catering contexts refer to the same event.
7. **Consider legacy updates and richer reporting later.** Resolve field meaning, currency, identifiers, and formula semantics before enabling any targeted update. Preview formula repairs independently. Scheduled reports must state their source coverage and unresolved costs; wedding and household costs stay separate by default.

### Verification performed

- Parsed all workbook, worksheet, shared-string, style, relationship, and drawing parts using Python's read-only ZIP/XML facilities. Inspected stored cells, formulas, cached values, merges, hidden rows, number formats, and available native features.
- Checked ZIP member integrity. Independently reconciled all eight `SUM` formula caches using exact decimal arithmetic, both prewedding option totals using the stated text amounts, and the left guest-block declared counts.
- Confirmed the file checksum after analysis. No source export, recalculation, formula execution, visual layout certification, Google API request, live runtime test, or workbook modification was performed.

### Decisions still needed before the affected implementation

The actual currency, meaning of E4/E7 and household D5, budget cost basis, current prewedding location quote, guest-estimate scenarios, planning year, authoritative live source, and legacy record identity remain unresolved. The assessment does not require resolving all of these to build safe read-only views: affected answers can preserve ambiguity. Resolve each prerequisite before enabling the operation that depends on it.
