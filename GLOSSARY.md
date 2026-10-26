# Personal Secretary glossary

## Language

**Change preview**: The exact proposed edit, including its target, previous values, resulting values, owner, and expiry. Approval applies only to that preview.

**Operation**: One requested spreadsheet mutation identified across approval, execution, verification, and reconciliation.
_Avoid_: Transaction (Sheets cannot guarantee a cross-sheet transaction).

**Reconciliation**: Resolving an uncertain operation from persisted intent and observed source evidence without resending it.

**Cost group**: A shared budget amount associated with one or more source items. Its amount is counted once.

**Reminder cancellation**: Disabling a native job while retaining its identity and available execution history. It remains canceled until an explicit resume.
