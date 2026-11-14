use super::{check_environment, ReadOnlyRuntimeSnapshot};
use crate::{
    domain::service_admission::{Plan, Promotion, Run, Supervisor},
    infrastructure::{
        config,
        service_check::{sensitive, FileKind, Files, ReadOnlySnapshot, Stamp},
        service_release,
    },
};
use serde::de::DeserializeOwned;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) mod qualification;

const MACHINE_ID: &str = "/etc/machine-id";

pub(super) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn authority(info: Stamp) -> (u64, u64, u32, u32, u32) {
    (info.device, info.inode, info.uid, info.gid, info.mode)
}

fn file_binding(info: Stamp) -> serde_json::Value {
    json!({"authority":authority(info), "size":info.size, "modified":info.modified, "changed":info.changed})
}

pub(super) fn load_snapshot(
    manifest: &Path,
    plugin_directory: &Path,
) -> Result<ReadOnlySnapshot, &'static str> {
    check_environment()?;
    let mut snapshot = ReadOnlySnapshot::load(manifest)?;
    snapshot.check_current_process()?;
    service_release::check(&mut snapshot, plugin_directory)?;
    snapshot.files.confirm()?;
    Ok(snapshot)
}

pub(super) fn protected<T: DeserializeOwned>(
    snapshot: &mut ReadOnlySnapshot,
    path: &Path,
) -> Result<(T, String), &'static str> {
    let name = path.to_str().ok_or("admission_file_invalid")?;
    let (_, _, _, policy) = snapshot.gateway_binding()?;
    let core = policy.core;
    let info = snapshot.files.open(name, FileKind::Read)?;
    sensitive(info, core)?;
    if info.size > 65_536 {
        return Err("admission_file_too_large");
    }
    let bytes = snapshot.files.read(name)?;
    if bytes.len() > 65_536 {
        return Err("admission_file_too_large");
    }
    let decoded = config::decode(&bytes).map_err(|_| "admission_json_invalid")?;
    snapshot.files.confirm()?;
    Ok((decoded, sha256(&bytes)))
}

pub(super) fn binding(
    prepared: &mut ReadOnlyRuntimeSnapshot,
    plugin_directory: &Path,
) -> Result<String, &'static str> {
    prepared.snapshot.files.open(MACHINE_ID, FileKind::Read)?;
    let machine_id = prepared.snapshot.files.read(MACHINE_ID)?;
    let machine_id = std::str::from_utf8(&machine_id).map_err(|_| "machine_identity_invalid")?;
    let machine_id = machine_id.strip_suffix('\n').unwrap_or(machine_id);
    if machine_id.len() != 32
        || machine_id.bytes().all(|b| b == b'0')
        || !machine_id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("machine_identity_invalid");
    }
    let (generation, _, policy_digest, _) = prepared.snapshot.gateway_binding()?;
    let inputs = prepared.borrowed_inputs()?;
    let manifest = &prepared.snapshot.checked.manifest;
    let mut protected_files = std::collections::BTreeMap::new();
    for name in [
        &manifest.owner_file,
        &manifest.core_token_file,
        &manifest.operator_token_file,
        &manifest.google_credentials_file,
        &manifest.budget_policy_file,
        &manifest.registry_file,
        &manifest.service_policy_file,
        &manifest.release_file,
    ] {
        let info = prepared.snapshot.files.input_stamp(name)?;
        protected_files.insert(name, file_binding(info));
    }
    let state = prepared
        .snapshot
        .files
        .input_stamp(&manifest.state_directory)?;
    let (sheets, docs) = super::readable_sources(inputs.registry);
    let mut value = json!({
        "domain": "ciel.readonly.effective-binding.v1",
        "profile": "readonly",
        "generation": generation,
        "machine": machine_id,
        "registry": inputs.registry,
        "owner": inputs.owner,
        "google_principal": inputs.google_principal,
        "sheets_read_scope": sheets,
        "docs_read_scope": docs,
        "budget_policy": inputs.budget_policy,
        "policy_sha256": policy_digest,
        "gateway_plan_sha256": prepared.gateway_digest,
        "release_sha256": prepared.snapshot.checked.release_digest,
        "plugin_directory": plugin_directory,
        "protected_files": protected_files,
        "state_directory": manifest.state_directory,
        "state_authority": authority(state),
        "listener": "127.0.0.1:8787",
        "sheets_endpoint": "https://sheets.googleapis.com/v4",
        "docs_endpoint": "https://docs.googleapis.com/v1",
        "token_endpoint": "https://oauth2.googleapis.com/token"
    });
    value.sort_all_objects();
    let encoded = serde_json::to_vec(&value).map_err(|_| "admission_binding_invalid")?;
    prepared.check_pre_runtime()?;
    Ok(sha256(&encoded))
}

pub(super) fn authorize(
    mut snapshot: ReadOnlySnapshot,
    gateway_plan: &Path,
    plugin_directory: &Path,
    promotion_path: &Path,
) -> Result<ReadOnlyRuntimeSnapshot, &'static str> {
    let (promotion, _): (Promotion, _) = protected(&mut snapshot, promotion_path)?;
    let (run, run_digest): (Run, _) = protected(&mut snapshot, Path::new(&promotion.run_file))?;
    let (plan, plan_digest): (Plan, _) = protected(&mut snapshot, Path::new(&promotion.plan_file))?;
    let (supervisor, supervisor_digest): (Supervisor, _) =
        protected(&mut snapshot, Path::new(&promotion.supervisor_file))?;
    if promotion.run_sha256 != run_digest
        || run.plan_sha256 != plan_digest
        || promotion.supervisor_sha256 != supervisor_digest
    {
        return Err("admission_evidence_digest");
    }
    supervisor.validate(&run, &run_digest)?;
    promotion.validate_evidence(
        &run,
        &plan,
        snapshot.registry()?,
        snapshot.deployment_generation()?,
        chrono::Utc::now().timestamp(),
    )?;
    for (observation, case) in run.observations.iter().zip(&plan.cases) {
        if observation.request_sha256 != qualification::request_digest(case) {
            return Err("admission_evidence_digest");
        }
    }
    let mut prepared = ReadOnlyRuntimeSnapshot::prepare(snapshot, gateway_plan)?;
    let digest = binding(&mut prepared, plugin_directory)?;
    promotion.check_binding(&digest)?;
    if promotion.expires_at <= chrono::Utc::now().timestamp() {
        return Err("promotion_invalid");
    }
    Ok(prepared)
}
