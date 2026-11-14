//! Evidence and promotion contracts for the read-only Core.
use super::{registry::Registry, service_config::revision};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

pub(crate) const MAX_CASES: usize = 20;
pub(crate) const MAX_SECONDS: u64 = 180;
pub(crate) const MAX_RESPONSE: usize = 2 * 1024 * 1024;
pub(crate) const MAX_CAPTURE_TOTAL: usize = 8 * 1024 * 1024;
const DAY: i64 = 86_400;

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Case {
    MissingBearer,
    WrongBearer,
    ReadonlyMutation,
    Sheet { spreadsheet: String, view: String },
    Document { document: String, tab_id: String },
}

impl Case {
    pub(crate) fn request(&self) -> (&'static str, serde_json::Value, u16) {
        use serde_json::json;
        match self {
            Self::MissingBearer | Self::WrongBearer => ("sheets_list", json!({}), 401),
            Self::ReadonlyMutation => (
                "sheets_create_tab",
                json!({"operation_id":"qualification-denied"}),
                403,
            ),
            Self::Sheet { spreadsheet, view } => (
                "sheets_read",
                json!({"spreadsheet":spreadsheet,"view":view,"limit":1}),
                200,
            ),
            Self::Document { document, tab_id } => (
                "docs_read",
                json!({"document":document,"tab_id":tab_id,"limit":1}),
                200,
            ),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Plan {
    pub(crate) schema: String,
    pub(crate) deployment_generation: String,
    pub(crate) wall_seconds: u64,
    pub(crate) cases: Vec<Case>,
}

impl Plan {
    pub(crate) fn validate(
        &self,
        generation: &str,
        registry: &Registry,
    ) -> Result<(), &'static str> {
        if self.schema != "ciel.readonly.qualification-plan.v1"
            || self.deployment_generation != generation
            || !(1..=MAX_SECONDS).contains(&self.wall_seconds)
        {
            return Err("qualification_plan_invalid");
        }
        check_coverage(&self.cases, registry)
    }
}

fn check_coverage(cases: &[Case], registry: &Registry) -> Result<(), &'static str> {
    if cases.len() > MAX_CASES
        || !cases.starts_with(&[
            Case::MissingBearer,
            Case::WrongBearer,
            Case::ReadonlyMutation,
        ])
    {
        return Err("qualification_cases_invalid");
    }

    let mut sheets: BTreeSet<_> = registry
        .spreadsheets
        .iter()
        .filter(|sheet| sheet.readable)
        .flat_map(|sheet| sheet.views.iter().map(|view| (&sheet.alias, &view.name)))
        .collect();
    let mut docs: BTreeSet<_> = registry
        .documents
        .iter()
        .filter(|doc| doc.readable)
        .map(|doc| &doc.alias)
        .collect();
    for case in &cases[3..] {
        let covered = match case {
            Case::Sheet { spreadsheet, view } => sheets.remove(&(spreadsheet, view)),
            Case::Document { document, tab_id } => {
                super::documents::valid_tab(tab_id) && docs.remove(document)
            }
            _ => false,
        };
        if !covered {
            return Err("qualification_cases_invalid");
        }
    }
    if cases.len() == 3 || !sheets.is_empty() || !docs.is_empty() {
        return Err("qualification_coverage_incomplete");
    }
    Ok(())
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvidenceClass {
    LocalSimulation,
    OperatorLiveCore,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    pub(crate) case: Case,
    pub(crate) status: u16,
    pub(crate) request_sha256: String,
    pub(crate) response_sha256: String,
    pub(crate) response_bytes: usize,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Unchecked {
    PluginAuthority,
    HostFootprint,
    NativeMemory,
    Approval,
    Scheduler,
    Delivery,
    ProviderWrites,
}

pub(crate) const UNCHECKED: [Unchecked; 7] = [
    Unchecked::PluginAuthority,
    Unchecked::HostFootprint,
    Unchecked::NativeMemory,
    Unchecked::Approval,
    Unchecked::Scheduler,
    Unchecked::Delivery,
    Unchecked::ProviderWrites,
];

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Run {
    pub(crate) schema: String,
    pub(crate) evidence_class: EvidenceClass,
    pub(crate) run_id: String,
    pub(crate) instance_pid: u32,
    pub(crate) deployment_generation: String,
    pub(crate) binding: String,
    pub(crate) plan_sha256: String,
    pub(crate) started_at: i64,
    pub(crate) finished_at: i64,
    pub(crate) elapsed_ms: u64,
    pub(crate) core_version: String,
    pub(crate) observations: Vec<Observation>,
    pub(crate) not_checked: Vec<Unchecked>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Promotion {
    pub(crate) schema: String,
    pub(crate) deployment_generation: String,
    pub(crate) binding: String,
    pub(crate) run_sha256: String,
    pub(crate) run_file: String,
    pub(crate) plan_file: String,
    pub(crate) supervisor_file: String,
    pub(crate) supervisor_sha256: String,
    pub(crate) reviewer: String,
    pub(crate) reviewed_at: i64,
    pub(crate) expires_at: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Supervisor {
    schema: String,
    run_sha256: String,
    plan_sha256: String,
    bundle_sha256: String,
    instance_pid: u32,
    exit_status: u16,
    port_released: bool,
}

impl Supervisor {
    pub(crate) fn validate(&self, run: &Run, run_digest: &str) -> Result<(), &'static str> {
        if self.schema != "ciel.readonly.supervisor.v1"
            || self.run_sha256 != run_digest
            || self.plan_sha256 != run.plan_sha256
            || self.instance_pid != run.instance_pid
            || self.exit_status != 0
            || !self.port_released
            || !digest(&self.bundle_sha256)
        {
            return Err("qualification_supervisor_invalid");
        }
        Ok(())
    }
}

pub(crate) fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Promotion {
    pub(crate) fn check_binding(&self, binding: &str) -> Result<(), &'static str> {
        if self.binding == binding {
            Ok(())
        } else {
            Err("promotion_binding")
        }
    }

    pub(crate) fn validate_evidence(
        &self,
        run: &Run,
        plan: &Plan,
        registry: &Registry,
        generation: &str,
        now: i64,
    ) -> Result<(), &'static str> {
        if self.schema != "ciel.readonly.promotion.v1"
            || self.deployment_generation != generation
            || !digest(&self.binding)
            || !revision(&self.reviewer)
            || self.reviewed_at > now
            || self.expires_at <= now
            || self.expires_at.saturating_sub(self.reviewed_at) > 30 * DAY
        {
            return Err("promotion_invalid");
        }
        plan.validate(generation, registry)?;
        if run.schema != "ciel.readonly.run.v1"
            || run.evidence_class != EvidenceClass::OperatorLiveCore
            || run.deployment_generation != generation
            || run.binding != self.binding
            || !Uuid::parse_str(&run.run_id)
                .is_ok_and(|id| id.get_version_num() == 4 && id.to_string() == run.run_id)
            || run.instance_pid == 0
            || run.core_version != env!("CARGO_PKG_VERSION")
            || run.started_at <= 0
            || run.finished_at < run.started_at
            || run.finished_at.saturating_sub(run.started_at) > plan.wall_seconds as i64 + 1
            || run.finished_at > self.reviewed_at
            || self.reviewed_at.saturating_sub(run.finished_at) > DAY
            || run.elapsed_ms > plan.wall_seconds * 1000
            || run.observations.len() != plan.cases.len()
            || run.not_checked != UNCHECKED
        {
            return Err("qualification_evidence_invalid");
        }
        let mut total = 0;
        for (observed, expected) in run.observations.iter().zip(&plan.cases) {
            if observed.case != *expected
                || observed.status != expected.request().2
                || !digest(&observed.request_sha256)
                || !digest(&observed.response_sha256)
                || !(1..=MAX_RESPONSE).contains(&observed.response_bytes)
            {
                return Err("qualification_evidence_invalid");
            }
            total += observed.response_bytes;
        }
        if total > MAX_CAPTURE_TOTAL {
            return Err("qualification_evidence_invalid");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Registry, Plan, Run, Promotion) {
        let registry =
            serde_json::from_str(include_str!("../../../../config/workspace.example.json"))
                .unwrap();
        let plan = Plan {
            schema: "ciel.readonly.qualification-plan.v1".into(),
            deployment_generation: "g1".into(),
            wall_seconds: 30,
            cases: vec![
                Case::MissingBearer,
                Case::WrongBearer,
                Case::ReadonlyMutation,
                Case::Sheet {
                    spreadsheet: "operations".into(),
                    view: "overview".into(),
                },
                Case::Document {
                    document: "notes".into(),
                    tab_id: "t.0".into(),
                },
            ],
        };
        let run = Run {
            schema: "ciel.readonly.run.v1".into(),
            evidence_class: EvidenceClass::OperatorLiveCore,
            run_id: Uuid::new_v4().to_string(),
            instance_pid: 19,
            deployment_generation: "g1".into(),
            binding: "a".repeat(64),
            plan_sha256: "b".repeat(64),
            started_at: 900,
            finished_at: 930,
            elapsed_ms: 30_000,
            core_version: env!("CARGO_PKG_VERSION").into(),
            not_checked: UNCHECKED.to_vec(),
            observations: plan
                .cases
                .iter()
                .map(|case| Observation {
                    case: case.clone(),
                    status: case.request().2,
                    request_sha256: "c".repeat(64),
                    response_sha256: "d".repeat(64),
                    response_bytes: 200,
                })
                .collect(),
        };
        let promotion = Promotion {
            schema: "ciel.readonly.promotion.v1".into(),
            deployment_generation: "g1".into(),
            binding: run.binding.clone(),
            run_sha256: "e".repeat(64),
            run_file: "/etc/ciel/run.json".into(),
            plan_file: "/etc/ciel/plan.json".into(),
            supervisor_file: "/etc/ciel/supervisor.json".into(),
            supervisor_sha256: "f".repeat(64),
            reviewer: "administrator".into(),
            reviewed_at: 940,
            expires_at: 1_200,
        };
        (registry, plan, run, promotion)
    }

    #[test]
    fn promotion_requires_complete_live_evidence_with_current_binding_and_order() {
        let (registry, mut plan, mut run, promotion) = fixture();
        assert_eq!(promotion.check_binding(&"a".repeat(64)), Ok(()));
        assert_eq!(
            promotion.validate_evidence(&run, &plan, &registry, "g1", 1_000),
            Ok(())
        );
        assert_eq!(
            promotion.validate_evidence(&run, &plan, &registry, "g2", 1_000),
            Err("promotion_invalid")
        );
        assert_eq!(promotion.check_binding("new"), Err("promotion_binding"));
        assert_eq!(
            promotion.validate_evidence(&run, &plan, &registry, "g1", 1_200),
            Err("promotion_invalid")
        );
        run.evidence_class = EvidenceClass::LocalSimulation;
        assert_eq!(
            promotion.validate_evidence(&run, &plan, &registry, "g1", 1_000),
            Err("qualification_evidence_invalid")
        );
        run.evidence_class = EvidenceClass::OperatorLiveCore;
        run.observations.swap(0, 1);
        assert_eq!(
            promotion.validate_evidence(&run, &plan, &registry, "g1", 1_000),
            Err("qualification_evidence_invalid")
        );
        plan.cases.pop();
        assert_eq!(
            plan.validate("g1", &registry),
            Err("qualification_coverage_incomplete")
        );
    }

    #[test]
    fn supervisor_must_observe_exact_child_exit_and_port_release() {
        let (_, _, run, promotion) = fixture();
        assert!(promotion.run_file.ends_with("run.json"));
        assert!(promotion.plan_file.ends_with("plan.json"));
        assert!(promotion.supervisor_file.ends_with("supervisor.json"));
        assert!(digest(&promotion.supervisor_sha256));
        let mut observation = Supervisor {
            schema: "ciel.readonly.supervisor.v1".into(),
            run_sha256: promotion.run_sha256.clone(),
            plan_sha256: run.plan_sha256.clone(),
            bundle_sha256: "a".repeat(64),
            instance_pid: run.instance_pid,
            exit_status: 0,
            port_released: true,
        };
        assert!(observation.validate(&run, &promotion.run_sha256).is_ok());
        assert!(observation.validate(&run, "different").is_err());
        observation.exit_status = 130;
        assert!(observation.validate(&run, &promotion.run_sha256).is_err());
        observation.exit_status = 0;
        observation.port_released = false;
        assert!(observation.validate(&run, &promotion.run_sha256).is_err());
    }
}
