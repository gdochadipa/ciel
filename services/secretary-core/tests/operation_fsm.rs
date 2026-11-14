//! Exhaustive matrix from the versioned durable-operation contract.
use secretary_core::domain::operation::{OperationEvent as E, OperationState as S, Outcome as O};

#[test]
fn all_state_event_pairs_obey_the_documented_transition_contract() {
    let states = [
        S::Previewed,
        S::Approved,
        S::Rejected,
        S::Expired,
        S::Applying,
        S::Applied,
        S::KnownFailure,
        S::Uncertain,
        S::Conflict,
        S::Reconciliation,
    ];
    let events = [
        ("approve", E::Approve),
        ("reject", E::Reject),
        ("expire", E::Expire),
        ("begin", E::BeginApply),
        ("recover", E::Recover),
        ("finish_applied", E::Finish(O::Applied)),
        ("finish_failure", E::Finish(O::KnownFailure)),
        ("finish_uncertain", E::Finish(O::Uncertain)),
        ("finish_conflict", E::Finish(O::Conflict)),
        ("finish_reconciliation", E::Finish(O::Reconciliation)),
    ];
    // Explicit allowed triples; absent cells are denials, including every
    // terminal execution outcome. Approve/Reject idempotency is deliberately narrow.
    let allowed = [
        (S::Previewed, "approve", S::Approved),
        (S::Approved, "approve", S::Approved),
        (S::Previewed, "reject", S::Rejected),
        (S::Rejected, "reject", S::Rejected),
        (S::Previewed, "expire", S::Expired),
        (S::Approved, "expire", S::Expired),
        (S::Approved, "begin", S::Applying),
        (S::Applying, "recover", S::Reconciliation),
        (S::Applying, "finish_applied", S::Applied),
        (S::Applying, "finish_failure", S::KnownFailure),
        (S::Applying, "finish_uncertain", S::Uncertain),
        (S::Applying, "finish_conflict", S::Conflict),
        (S::Applying, "finish_reconciliation", S::Reconciliation),
        (S::Uncertain, "finish_applied", S::Applied),
        (S::Uncertain, "finish_conflict", S::Conflict),
        (S::Uncertain, "finish_reconciliation", S::Reconciliation),
        (S::Reconciliation, "finish_applied", S::Applied),
        (S::Reconciliation, "finish_conflict", S::Conflict),
        (
            S::Reconciliation,
            "finish_reconciliation",
            S::Reconciliation,
        ),
    ];
    let mut accepted = 0;
    for state in states {
        for (name, event) in events {
            match allowed
                .iter()
                .find(|(before, action, _)| *before == state && *action == name)
            {
                Some((_, _, expected)) => {
                    assert_eq!(
                        state.transition(event).unwrap(),
                        *expected,
                        "{state:?}/{name}"
                    );
                    accepted += 1;
                }
                None => assert_eq!(
                    state.transition(event).unwrap_err().1,
                    "operation_conflict",
                    "{state:?}/{name}"
                ),
            }
        }
    }
    assert_eq!(accepted, allowed.len());
}
