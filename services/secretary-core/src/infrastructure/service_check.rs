//! Offline snapshots only: no OAuth, journal, listener, process launch or secrets.
use super::{config, service_files::SafeFiles};
use crate::domain::{
    registry::Registry,
    service_config::{absolute_path, Manifest, Principal, ServicePolicy},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileKind {
    Read,
    Metadata,
    State,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Stamp {
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
    pub device: u64,
    pub inode: u64,
    pub links: u64,
    pub directory: bool,
    pub size: u64,
    pub modified: (i64, i64),
    pub changed: (i64, i64),
}

pub(super) trait Files {
    fn open(&mut self, path: &str, kind: FileKind) -> Result<Stamp, &'static str>;
    fn read(&mut self, path: &str) -> Result<Vec<u8>, &'static str>;
    fn hash(&mut self, path: &str) -> Result<String, &'static str>;
    fn directories(&self) -> Vec<Stamp>;
    fn confirm(&self) -> Result<(), &'static str>;
}

/// Immutable non-secret configuration and held file-authority observations.
///
/// Construction uses the same protected loader as the offline CLI. This object
/// is deliberately not deserializable, clonable or printable: a caller-supplied
/// policy/registry is not evidence of administrator-controlled file authority.
/// It never grants runtime admission or loads owner/token/key/budget bodies.
///
/// # Examples
///
/// ```no_run
/// use secretary_core::infrastructure::service_check::ReadOnlySnapshot;
/// let snapshot = ReadOnlySnapshot::load(std::path::Path::new("/etc/secretary/readonly/manifest.json"))?;
/// let _registry = snapshot.registry()?;
/// let _generation = snapshot.deployment_generation()?;
/// snapshot.check_current_process()?; // A prerequisite, not permission to start.
/// assert_eq!(snapshot.report()?["readiness"], "incomplete");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct ReadOnlySnapshot {
    pub(super) checked: CheckedConfiguration,
    pub(super) files: SafeFiles,
}

pub(super) struct CheckedConfiguration {
    pub(super) manifest: Manifest,
    policy: ServicePolicy,
    registry: Registry,
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    pub(super) release: Release,
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    pub(super) release_digest: String,
    #[cfg(target_os = "linux")]
    manifest_digest: String,
    #[cfg(target_os = "linux")]
    policy_digest: String,
}

impl ReadOnlySnapshot {
    #[cfg(target_os = "linux")]
    pub(super) fn gateway_binding(
        &self,
    ) -> Result<(&str, &str, &str, &ServicePolicy), &'static str> {
        self.files.confirm()?;
        Ok((
            &self.checked.manifest.deployment_generation,
            &self.checked.manifest_digest,
            &self.checked.policy_digest,
            &self.checked.policy,
        ))
    }

    /// Check a root-published Gateway launch intent and its current kernel identity.
    ///
    /// This point-in-time prerequisite does not prove exec, plugin activation or
    /// admission. The protected plan path is an operator input, never tool input.
    /// Call from the actual single-threaded Core before secrets or runtime threads.
    ///
    /// # Examples
    /// ```no_run
    /// use secretary_core::infrastructure::service_check::ReadOnlySnapshot;
    /// let snapshot = ReadOnlySnapshot::load(std::path::Path::new("/etc/ciel/manifest.json"))?;
    /// snapshot.check_gateway_process(std::path::Path::new("/etc/ciel/gateway.json"))?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// Returns a sanitized code for unsafe/stale input, process, mapping or namespace.
    pub fn check_gateway_process(&self, plan: &Path) -> Result<(), &'static str> {
        self.check_current_process()?;
        #[cfg(target_os = "linux")]
        super::gateway_launch::check_gateway(self, plan)?;
        #[cfg(not(target_os = "linux"))]
        {
            let _ = plan;
            Err("launcher_authority_unsupported")
        }
        #[cfg(target_os = "linux")]
        self.check_current_process()
    }
    /// Load one protected, statically validated snapshot without runtime effects.
    ///
    /// # Errors
    /// Returns sanitized codes for invalid/unsafe inputs or unsupported platforms.
    pub fn load(path: &Path) -> Result<Self, &'static str> {
        let path = path
            .to_str()
            .filter(|path| absolute_path(path))
            .ok_or("manifest_path")?;
        let mut files = SafeFiles::new();
        let checked = load_files(path, &mut files)?;
        Ok(Self { checked, files })
    }

    /// Borrow the validated registry, rechecking held and current file metadata.
    ///
    /// A copied registry is data, not a transferable admission capability. Future
    /// runtime composition must keep the snapshot and recheck before consumption.
    ///
    /// # Errors
    /// Returns a sanitized code if any input, ancestor or authority has changed.
    pub fn registry(&self) -> Result<&Registry, &'static str> {
        self.files.confirm()?;
        Ok(&self.checked.registry)
    }

    /// Borrow the administrator-selected generation for future private binding.
    ///
    /// This value is not included in sanitized offline reports.
    ///
    /// # Errors
    /// Returns a sanitized code if the protected snapshot has changed.
    pub fn deployment_generation(&self) -> Result<&str, &'static str> {
        self.files.confirm()?;
        Ok(&self.checked.manifest.deployment_generation)
    }

    /// Check this process against the policy loaded in this protected snapshot.
    ///
    /// Call in the future Core's actual process before worker threads or secrets.
    /// Release compatibility, runtime secrets and admission are checked separately.
    ///
    /// # Errors
    /// Returns sanitized file-change or process-authority denial codes.
    pub fn check_current_process(&self) -> Result<(), &'static str> {
        self.files.confirm()?;
        super::service_launcher::check_snapshot_process(&self.checked.policy, &self.files)?;
        self.files.confirm()
    }

    /// Return the existing sanitized incomplete report after metadata revalidation.
    ///
    /// # Errors
    /// Returns a sanitized code if the protected snapshot has changed.
    pub fn report(&self) -> Result<Value, &'static str> {
        self.files.confirm()?;
        Ok(self.checked.report())
    }
}

/// Inspect a candidate without opening secret bodies or changing runtime state.
///
/// A structurally valid candidate remains incomplete while budget, launcher,
/// release admission and runtime-secret checks are outstanding.
///
/// # Examples
///
/// ```no_run
/// use secretary_core::infrastructure::service_check::inspect;
/// let report = inspect(std::path::Path::new("/etc/secretary/readonly/manifest.json"))?;
/// assert_eq!(report["readiness"], "incomplete");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
/// Returns sanitized codes for invalid input, unsupported platform or unsafe files.
pub fn inspect(path: &Path) -> Result<Value, &'static str> {
    ReadOnlySnapshot::load(path)?.report()
}

fn decode<T: serde::de::DeserializeOwned>(
    files: &mut impl Files,
    path: &str,
) -> Result<T, &'static str> {
    config::decode(&files.read(path)?).map_err(|_| "configuration_json_invalid")
}

pub(super) fn sensitive(info: Stamp, core: Principal) -> Result<(), &'static str> {
    if info.directory
        || info.uid != 0
        || info.gid != core.gid
        || info.links != 1
        || !matches!(info.mode, 0o440 | 0o640)
    {
        return Err("private_file_policy");
    }
    Ok(())
}

fn distinct(seen: &mut BTreeSet<(u64, u64)>, info: Stamp) -> Result<(), &'static str> {
    if !seen.insert((info.device, info.inode)) {
        return Err("input_alias");
    }
    Ok(())
}

#[cfg(test)]
fn inspect_files(path: &str, files: &mut impl Files) -> Result<Value, &'static str> {
    Ok(load_files(path, files)?.report())
}

fn load_files(path: &str, files: &mut impl Files) -> Result<CheckedConfiguration, &'static str> {
    let manifest_info = files.open(path, FileKind::Read)?;
    let (manifest, manifest_digest): (Manifest, _) = decode_hashed(files, path)?;
    manifest.validate()?;
    if manifest.files().contains(&path)
        || manifest.state_directory == path
        || path.starts_with(&format!("{}/", manifest.state_directory))
    {
        return Err("manifest_path_alias");
    }
    let policy_info = files.open(&manifest.service_policy_file, FileKind::Read)?;
    let (policy, policy_digest): (ServicePolicy, _) =
        decode_hashed(files, &manifest.service_policy_file)?;
    policy.validate()?;
    sensitive(manifest_info, policy.core)?;
    sensitive(policy_info, policy.core)?;
    let mut seen = BTreeSet::new();
    distinct(&mut seen, manifest_info)?;
    distinct(&mut seen, policy_info)?;
    let mut inputs = vec![manifest_info, policy_info];

    // These bodies remain unopened, even when they contain invalid tokens/keys.
    // Budget policy is a runtime-only input; offline inspection never opens it.
    for name in [
        &manifest.owner_file,
        &manifest.core_token_file,
        &manifest.operator_token_file,
        &manifest.google_credentials_file,
        &manifest.budget_policy_file,
    ] {
        let info = files.open(name, FileKind::Metadata)?;
        sensitive(info, policy.core)?;
        distinct(&mut seen, info)?;
        inputs.push(info);
    }
    let registry_info = files.open(&manifest.registry_file, FileKind::Read)?;
    sensitive(registry_info, policy.core)?;
    distinct(&mut seen, registry_info)?;
    inputs.push(registry_info);
    let registry: Registry = decode(files, &manifest.registry_file)?;
    registry
        .validate_read_only()
        .map_err(|_| "registry_readonly_invalid")?;
    let sheets = registry
        .spreadsheets
        .iter()
        .filter(|source| source.readable)
        .count();
    let documents = registry
        .documents
        .iter()
        .filter(|source| source.readable)
        .count();
    if sheets + documents == 0 {
        return Err("registry_no_readable_resources");
    }
    let state = files.open(&manifest.state_directory, FileKind::State)?;
    if !state.directory
        || state.uid != policy.core.uid
        || state.gid != policy.core.gid
        || state.mode != 0o700
    {
        return Err("private_state_policy");
    }
    distinct(&mut seen, state)?;
    let groups = local_accounts(files, &policy)?;

    let release_info = files.open(&manifest.release_file, FileKind::Read)?;
    distinct(&mut seen, release_info)?;
    inputs.push(release_info);
    let (release, release_digest): (Release, _) = decode_hashed(files, &manifest.release_file)?;
    release.validate()?;
    let parent = Path::new(&manifest.release_file)
        .parent()
        .and_then(Path::to_str)
        .ok_or("release_invalid")?;
    let mut artifact_bytes = 0u64;
    for (name, expected) in &release.artifact_hashes {
        let artifact = Path::new(parent).join(name);
        let name = artifact.to_str().ok_or("release_invalid")?.to_owned();
        let info = files.open(&name, FileKind::Read)?;
        distinct(&mut seen, info)?;
        if info.uid != 0 || info.links != 1 || info.directory || info.mode & 0o7022 != 0 {
            return Err("release_file_policy");
        }
        if name.ends_with("/bin/secretary-core") && info.mode & 0o111 == 0 {
            return Err("release_not_executable");
        }
        artifact_bytes = artifact_bytes
            .checked_add(info.size)
            .ok_or("release_too_large")?;
        if artifact_bytes > 128 * 1024 * 1024 {
            return Err("release_too_large");
        }
        if files.hash(&name)? != *expected {
            return Err("release_hash_mismatch");
        }
        inputs.push(info);
    }
    for info in inputs {
        if !can_access(info, policy.core, &groups, 0o4) {
            return Err("service_file_access");
        }
    }
    for directory in files.directories() {
        if !can_access(directory, policy.core, &groups, 0o1) {
            return Err("service_directory_access");
        }
    }
    files.confirm()?;
    #[cfg(not(target_os = "linux"))]
    let _ = (manifest_digest, policy_digest);
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    let _ = release_digest;
    Ok(CheckedConfiguration {
        manifest,
        policy,
        registry,
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        release,
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        release_digest,
        #[cfg(target_os = "linux")]
        manifest_digest,
        #[cfg(target_os = "linux")]
        policy_digest,
    })
}

fn decode_hashed<T: serde::de::DeserializeOwned>(
    files: &mut impl Files,
    path: &str,
) -> Result<(T, String), &'static str> {
    use sha2::{Digest, Sha256};
    let bytes = files.read(path)?;
    let value = config::decode(&bytes).map_err(|_| "configuration_json_invalid")?;
    Ok((value, format!("{:x}", Sha256::digest(&bytes))))
}

impl CheckedConfiguration {
    fn report(&self) -> Value {
        let sheets = self
            .registry
            .spreadsheets
            .iter()
            .filter(|source| source.readable)
            .count();
        let documents = self
            .registry
            .documents
            .iter()
            .filter(|source| source.readable)
            .count();
        json!({
        "ok":false,"mode":"offline","readiness":"incomplete","profile":"readonly",
        "checks":{"manifest":"passed","service_policy":"passed","file_metadata":"passed",
            "local_account_groups":"passed","registry":"passed","release_hashes":"passed",
            "budget_policy":"unsupported","effective_launcher_groups":"pending",
            "release_contract_and_compatibility":"pending","runtime_secret_validation":"pending"},
        "summary":{"readable_spreadsheets":sheets,"readable_documents":documents},
        "blockers":["budget_policy_runtime_validation_pending","launcher_authority_pending",
            "release_contract_and_compatibility_pending","runtime_secret_validation_pending"],
        "configuration_changed":false,"activation_changed":false,"live_access_checked":false,
        "service_started":false,"journal_opened":false
        })
    }
}

fn can_access(info: Stamp, principal: Principal, groups: &BTreeSet<u32>, bit: u32) -> bool {
    let shift = if info.uid == principal.uid {
        6
    } else if groups.contains(&info.gid) {
        3
    } else {
        0
    };
    info.mode & (bit << shift) != 0
}

fn local_accounts(
    files: &mut impl Files,
    policy: &ServicePolicy,
) -> Result<BTreeSet<u32>, &'static str> {
    for name in ["/etc/passwd", "/etc/group"] {
        files.open(name, FileKind::Read)?;
    }
    let passwd =
        String::from_utf8(files.read("/etc/passwd")?).map_err(|_| "local_accounts_invalid")?;
    let groups =
        String::from_utf8(files.read("/etc/group")?).map_err(|_| "local_accounts_invalid")?;
    let accounts: Result<Vec<_>, _> = passwd
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let fields: Vec<_> = line.split(':').collect();
            if fields.len() != 7 || fields[0].is_empty() {
                return Err("local_accounts_invalid");
            }
            Ok((fields[0], decimal(fields[2])?, decimal(fields[3])?))
        })
        .collect();
    let accounts = accounts?;
    if accounts
        .iter()
        .map(|entry| entry.0)
        .collect::<BTreeSet<_>>()
        .len()
        != accounts.len()
        || accounts.iter().filter(|(_, uid, _)| *uid == 0).count() != 1
        || !accounts
            .iter()
            .any(|(name, uid, _)| *name == "root" && *uid == 0)
    {
        return Err("local_accounts_invalid");
    }
    let account = |principal: Principal| {
        let selected: Vec<_> = accounts
            .iter()
            .filter(|(_, uid, _)| *uid == principal.uid)
            .collect();
        match selected.as_slice() {
            [entry] if entry.2 == principal.gid => Ok(entry.0),
            _ => Err("local_identity_mismatch"),
        }
    };
    let core = account(policy.core)?;
    let gateway = account(policy.gateway)?;
    if core == gateway
        || accounts
            .iter()
            .any(|(_, uid, gid)| *uid != 0 && *uid != policy.core.uid && *gid == policy.core.gid)
    {
        return Err("service_group_not_exclusive");
    }
    let mut core_groups = BTreeSet::from([policy.core.gid]);
    let mut found = BTreeSet::new();
    for line in groups.lines().filter(|line| !line.is_empty()) {
        let fields: Vec<_> = line.split(':').collect();
        if fields.len() != 4 || fields[0].is_empty() {
            return Err("local_accounts_invalid");
        }
        let gid = decimal(fields[2])?;
        if !found.insert(gid) {
            return Err("local_accounts_invalid");
        }
        let members: Vec<_> = fields[3]
            .split(',')
            .filter(|name| !name.is_empty())
            .collect();
        if gid == policy.core.gid && members.iter().any(|name| *name != core && *name != "root") {
            return Err("service_group_not_exclusive");
        }
        if members.contains(&core) {
            core_groups.insert(gid);
        }
    }
    if !found.contains(&policy.core.gid) || !found.contains(&policy.gateway.gid) {
        return Err("local_identity_mismatch");
    }
    // Unit-level SupplementaryGroups or dynamic credentials are not proved by
    // local account files; launcher authority remains an explicit blocker.
    Ok(core_groups)
}

fn decimal(value: &str) -> Result<u32, &'static str> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("local_accounts_invalid");
    }
    value.parse().map_err(|_| "local_accounts_invalid")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Release {
    format_version: u32,
    source_commit: String,
    pub(super) build_host: BuildHost,
    pub(super) tool_versions: BTreeMap<String, String>,
    tool_count: u32,
    production_writes: bool,
    starts_services: bool,
    activates_plugins: bool,
    pub(super) lock_hashes: BTreeMap<String, String>,
    pub(super) artifact_hashes: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BuildHost {
    system: String,
    pub(super) machine: String,
    pub(super) libc: Vec<String>,
}

pub(super) const ARTIFACTS: [&str; 11] = [
    "LICENSE",
    "scripts/preflight.py",
    "scripts/backup.py",
    "templates/service.env.example",
    "templates/qualification.example.json",
    "templates/registry.example.json",
    "templates/openclaw.example.json",
    "templates/secretary-core.service",
    "INSTALLATION.md",
    "bin/secretary-core",
    "plugins/secretary-tools.tgz",
];

impl Release {
    fn validate(&self) -> Result<(), &'static str> {
        let hex = |value: &str, size| {
            value.len() == size
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if self.format_version != 2
            || !hex(&self.source_commit, 40)
            // Inventory of this release, not its effective read-only exposure.
            // Compatibility still requires the exact compiled plugin manifest.
            || self.tool_count != 33
            || self.production_writes
            || self.starts_services
            || self.activates_plugins
            || self.build_host.system != "Linux"
            || !matches!(self.build_host.machine.as_str(), "x86_64" | "aarch64")
            || self.build_host.libc.len() != 2
            || self
                .build_host
                .libc
                .iter()
                .any(|s| s.is_empty() || s.len() > 128)
            || self.tool_versions.is_empty()
            || self.tool_versions.len() > 16
            || self.tool_versions.iter().any(|(key, value)| {
                key.is_empty() || key.len() > 64 || value.is_empty() || value.len() > 256
            })
            || self
                .artifact_hashes
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                != BTreeSet::from(ARTIFACTS)
            || self
                .lock_hashes
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                != BTreeSet::from(["Cargo.lock", "bun.lock"])
            || self
                .artifact_hashes
                .values()
                .chain(self.lock_hashes.values())
                .any(|v| !hex(v, 64))
        {
            return Err("release_invalid");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    const MANIFEST: &str = "/etc/secretary/readonly/manifest.json";

    struct Fixture {
        entries: BTreeMap<String, (Stamp, Vec<u8>)>,
        opened: BTreeMap<String, FileKind>,
        read: BTreeSet<String>,
        changed: bool,
    }

    impl Fixture {
        fn new() -> Self {
            let manifest: Value = serde_json::from_str(include_str!(
                "../../../../config/readonly-manifest.example.json"
            ))
            .unwrap();
            let mut fixture = Self {
                entries: BTreeMap::new(),
                opened: BTreeMap::new(),
                read: BTreeSet::new(),
                changed: false,
            };
            fixture.json(MANIFEST, manifest.clone());
            for key in [
                "owner_file",
                "core_token_file",
                "operator_token_file",
                "google_credentials_file",
                "budget_policy_file",
            ] {
                fixture.insert(
                    manifest[key].as_str().unwrap(),
                    b"PRIVATE_UNREAD_SECRET_CANARY".to_vec(),
                );
            }
            fixture.json(
                manifest["service_policy_file"].as_str().unwrap(),
                serde_json::from_str(include_str!(
                    "../../../../config/service-policy.example.json"
                ))
                .unwrap(),
            );
            fixture.json(
                manifest["registry_file"].as_str().unwrap(),
                serde_json::from_str(include_str!("../../../../config/workspace.example.json"))
                    .unwrap(),
            );
            let state = manifest["state_directory"].as_str().unwrap();
            fixture.insert(state, Vec::new());
            let stamp = &mut fixture.entries.get_mut(state).unwrap().0;
            stamp.uid = 10001;
            stamp.mode = 0o700;
            stamp.directory = true;
            stamp.links = 2;
            fixture.insert("/etc/passwd", b"root:x:0:0::/:/bin/sh\ncore:x:10001:10001::/:/bin/false\ngateway:x:10002:10002::/:/bin/false\n".to_vec());
            fixture.insert(
                "/etc/group",
                b"root:x:0:\ncore:x:10001:\ngateway:x:10002:\n".to_vec(),
            );
            let artifact = b"synthetic artifact bytes";
            let hashes: BTreeMap<_, _> = ARTIFACTS
                .into_iter()
                .map(|name| (name, format!("{:x}", Sha256::digest(artifact))))
                .collect();
            for name in ARTIFACTS {
                let path = format!("/opt/secretary/{name}");
                fixture.insert(&path, artifact.to_vec());
                fixture.entries.get_mut(&path).unwrap().0.mode = if name == "bin/secretary-core" {
                    0o755
                } else {
                    0o644
                };
            }
            fixture.json(manifest["release_file"].as_str().unwrap(), json!({
                "format_version":2,"source_commit":"a".repeat(40),
                "build_host":{"system":"Linux","machine":"x86_64","libc":["glibc","2.39"]},
                "tool_versions":{"rustc":"synthetic"},"tool_count":33,
                "production_writes":false,"starts_services":false,"activates_plugins":false,
                "lock_hashes":{"Cargo.lock":"b".repeat(64),"bun.lock":"c".repeat(64)},"artifact_hashes":hashes
            }));
            fixture
        }

        fn insert(&mut self, path: &str, bytes: Vec<u8>) {
            let inode = self.entries.len() as u64 + 10;
            let info = Stamp {
                uid: 0,
                gid: 10001,
                mode: 0o640,
                device: 1,
                inode,
                links: 1,
                directory: false,
                size: bytes.len() as u64,
                modified: (1, 0),
                changed: (1, 0),
            };
            self.entries.insert(path.into(), (info, bytes));
        }

        fn json(&mut self, path: &str, value: Value) {
            self.insert(path, serde_json::to_vec(&value).unwrap());
        }

        fn change_json(&mut self, path: &str, change: impl FnOnce(&mut Value)) {
            let entry = self.entries.get_mut(path).unwrap();
            let mut value: Value = serde_json::from_slice(&entry.1).unwrap();
            change(&mut value);
            entry.1 = serde_json::to_vec(&value).unwrap();
            entry.0.size = entry.1.len() as u64;
        }

        fn inspect(&mut self) -> Result<Value, &'static str> {
            inspect_files(MANIFEST, self)
        }
    }

    impl Files for Fixture {
        fn open(&mut self, path: &str, kind: FileKind) -> Result<Stamp, &'static str> {
            if self.opened.insert(path.into(), kind).is_some() {
                return Err("input_alias");
            }
            self.entries
                .get(path)
                .map(|(stamp, _)| *stamp)
                .ok_or("file_unavailable")
        }
        fn read(&mut self, path: &str) -> Result<Vec<u8>, &'static str> {
            assert_eq!(
                self.opened.get(path),
                Some(&FileKind::Read),
                "secret/state body was accessed"
            );
            self.read.insert(path.into());
            Ok(self.entries.get(path).ok_or("file_unavailable")?.1.clone())
        }
        fn hash(&mut self, path: &str) -> Result<String, &'static str> {
            Ok(format!("{:x}", Sha256::digest(self.read(path)?)))
        }
        fn directories(&self) -> Vec<Stamp> {
            self.entries
                .values()
                .filter_map(|(info, _)| info.directory.then_some(*info))
                .collect()
        }
        fn confirm(&self) -> Result<(), &'static str> {
            if self.changed {
                Err("file_changed")
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn complete_static_snapshot_stays_incomplete_and_never_reads_secrets() {
        let mut fixture = Fixture::new();
        let report = fixture.inspect().unwrap();
        assert_eq!(report["ok"], false);
        assert_eq!(report["readiness"], "incomplete");
        assert_eq!(report["checks"]["registry"], "passed");
        assert_eq!(report["checks"]["release_hashes"], "passed");
        assert_eq!(
            report["blockers"],
            json!([
                "budget_policy_runtime_validation_pending",
                "launcher_authority_pending",
                "release_contract_and_compatibility_pending",
                "runtime_secret_validation_pending"
            ])
        );
        assert_eq!(report["service_started"], false);
        assert_eq!(report["journal_opened"], false);
        let output = report.to_string();
        assert!(!output.contains("PRIVATE"));
        assert!(!output.contains("/etc"));
        assert!(!output.contains("generic-spreadsheet"));
        for (path, kind) in &fixture.opened {
            if *kind != FileKind::Read {
                assert!(!fixture.read.contains(path));
            }
        }
    }

    #[test]
    fn invalid_or_duplicate_manifest_and_policy_are_rejected_without_defaulting() {
        for path in [MANIFEST, "/etc/secretary/readonly/service-policy.json"] {
            let mut fixture = Fixture::new();
            let entry = fixture.entries.get_mut(path).unwrap();
            let text = String::from_utf8(entry.1.clone()).unwrap();
            entry.1 = format!("{{\"schema\":\"PRIVATE_CANARY\",{}", &text[1..]).into_bytes();
            assert_eq!(fixture.inspect().unwrap_err(), "configuration_json_invalid");
        }
        let mut fixture = Fixture::new();
        fixture.change_json(MANIFEST, |value| value["force"] = json!(true));
        assert_eq!(fixture.inspect().unwrap_err(), "configuration_json_invalid");
    }

    #[test]
    fn dormant_write_grants_and_empty_read_sets_cannot_pass() {
        let mut fixture = Fixture::new();
        fixture.change_json("/etc/secretary/readonly/registry.json", |value| {
            value["documents"][0]["readable"] = json!(false);
            value["documents"][0]["writable"] = json!(true);
        });
        assert_eq!(fixture.inspect().unwrap_err(), "registry_readonly_invalid");
        let mut fixture = Fixture::new();
        fixture.change_json("/etc/secretary/readonly/registry.json", |value| {
            value["documents"] = json!([]);
            value["spreadsheets"] = json!([]);
        });
        assert_eq!(
            fixture.inspect().unwrap_err(),
            "registry_no_readable_resources"
        );
    }

    #[test]
    fn local_gateway_group_overlap_and_identity_mismatch_fail_closed() {
        for group in [
            b"root:x:0:\ncore:x:10001:gateway\ngateway:x:10002:\n".as_slice(),
            b"root:x:0:\ncore:x:10001:\n",
        ] {
            let mut fixture = Fixture::new();
            fixture.entries.get_mut("/etc/group").unwrap().1 = group.to_vec();
            assert!(matches!(
                fixture.inspect().unwrap_err(),
                "service_group_not_exclusive" | "local_identity_mismatch"
            ));
        }
        let mut fixture = Fixture::new();
        fixture.entries.get_mut("/etc/passwd").unwrap().1 =
            b"root:x:0:0::/:/bin/sh\ncore:x:999:10001::/:/bin/false\ngateway:x:10002:10002::/:/bin/false\n".to_vec();
        assert_eq!(fixture.inspect().unwrap_err(), "local_identity_mismatch");
        let mut fixture = Fixture::new();
        fixture.entries.get_mut("/etc/passwd").unwrap().1 =
            b"root:x:0:0::/:/bin/sh\nroot:x:10001:10001::/:/bin/false\ngateway:x:10002:10002::/:/bin/false\n".to_vec();
        assert_eq!(fixture.inspect().unwrap_err(), "local_accounts_invalid");
    }

    #[test]
    fn alias_permissions_and_concurrent_change_cannot_issue_a_static_report() {
        let mut fixture = Fixture::new();
        let inode = fixture.entries["/etc/secretary/readonly/core.token"]
            .0
            .inode;
        fixture
            .entries
            .get_mut("/etc/secretary/readonly/operator.token")
            .unwrap()
            .0
            .inode = inode;
        assert_eq!(fixture.inspect().unwrap_err(), "input_alias");
        let mut fixture = Fixture::new();
        fixture
            .entries
            .get_mut("/etc/secretary/readonly/google.json")
            .unwrap()
            .0
            .mode = 0o644;
        assert_eq!(fixture.inspect().unwrap_err(), "private_file_policy");
        let mut fixture = Fixture::new();
        fixture
            .entries
            .get_mut("/var/lib/secretary-readonly")
            .unwrap()
            .0
            .mode = 0o750;
        assert_eq!(fixture.inspect().unwrap_err(), "private_state_policy");
        let mut fixture = Fixture::new();
        fixture.changed = true;
        assert_eq!(fixture.inspect().unwrap_err(), "file_changed");
    }

    #[test]
    fn tampered_release_and_extra_inventory_entries_are_denied() {
        let mut fixture = Fixture::new();
        fixture
            .entries
            .get_mut("/opt/secretary/bin/secretary-core")
            .unwrap()
            .1 = b"changed".to_vec();
        assert_eq!(fixture.inspect().unwrap_err(), "release_hash_mismatch");
        let mut fixture = Fixture::new();
        fixture.change_json("/opt/secretary/release.json", |value| {
            value["artifact_hashes"]["../escape"] = json!("a".repeat(64))
        });
        assert_eq!(fixture.inspect().unwrap_err(), "release_invalid");
    }

    #[test]
    fn declared_tool_inventory_must_match_this_release_without_activating_it() {
        for count in [0, 26, 32, 34] {
            let mut fixture = Fixture::new();
            fixture.change_json("/opt/secretary/release.json", |value| {
                value["tool_count"] = json!(count)
            });
            assert_eq!(fixture.inspect().unwrap_err(), "release_invalid");
        }
        let mut fixture = Fixture::new();
        let report = fixture.inspect().unwrap();
        assert_eq!(report["checks"]["release_hashes"], "passed");
        assert_eq!(report["readiness"], "incomplete");
        assert_eq!(report["service_started"], false);
        assert_eq!(report["journal_opened"], false);
    }
}
