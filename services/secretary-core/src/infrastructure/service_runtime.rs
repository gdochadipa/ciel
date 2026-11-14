//! Protected bootstrap, qualification and admitted runtime composition.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod admission;
use super::{config, service_check::ReadOnlySnapshot};
use crate::domain::read_budget::ReadBudgetPolicy;
use crate::domain::registry::Registry;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

/// Consumed bootstrap proof and command-boundary lifetime checks; not admission.
///
/// # Examples
/// ```no_run
/// # use secretary_core::infrastructure::service_runtime::ReadOnlyRuntimeSnapshot;
/// # fn next(prepared: ReadOnlyRuntimeSnapshot) -> Result<(), &'static str> {
/// let authority = prepared.into_runtime()?;
/// authority.check_runtime()?;
/// # Ok(()) }
/// ```
pub struct ReadOnlyRuntimeAuthority {
    prepared: ReadOnlyRuntimeSnapshot,
    #[cfg(target_os = "linux")]
    context: super::gateway_launch::RuntimeContext,
    check_lock: Mutex<()>,
    fenced: AtomicBool,
}

impl ReadOnlyRuntimeAuthority {
    /// Check protected files, all Core/Gateway threads and current runtime handles.
    ///
    /// Any failure permanently fences this object. This creates no serving grant.
    ///
    /// # Errors
    /// Returns a sanitized denial; failed objects cannot be revived in place.
    pub fn check_runtime(&self) -> Result<(), &'static str> {
        let _lock = self.check_lock.lock().map_err(|_| "runtime_fenced")?;
        if self.fenced.load(Ordering::Acquire) {
            return Err("runtime_fenced");
        }
        let result = self.observe();
        if result.is_err() {
            self.fenced.store(true, Ordering::Release);
        }
        result
    }

    fn observe(&self) -> Result<(), &'static str> {
        check_environment()?;
        #[cfg(target_os = "linux")]
        {
            self.context
                .check(&self.prepared.snapshot, &self.prepared.gateway_plan)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err("launcher_authority_unsupported")
        }
    }

    /// Borrow the original configuration after a runtime authority observation.
    ///
    /// # Errors
    /// Returns a sanitized denial when lifetime authority no longer holds.
    pub fn inputs(&self) -> Result<RuntimeInputs<'_>, &'static str> {
        self.check_runtime()?;
        self.prepared.borrowed_inputs()
    }
}

impl crate::application::ports::RuntimeAuthority for ReadOnlyRuntimeAuthority {
    fn check(&self) -> Result<(), &'static str> {
        self.check_runtime()
    }
}

/// Protected static inputs and once-read validated secrets for future composition.
///
/// This deliberately has no Debug/Clone/serialization implementation. It is not
/// an admission capability: release compatibility and trusted host-owner routing
/// remain separate gates. Runtime enforcement requires the consuming transition.
///
/// # Examples
/// ```no_run
/// use secretary_core::infrastructure::{service_check::ReadOnlySnapshot,
///     service_runtime::ReadOnlyRuntimeSnapshot};
/// let snapshot = ReadOnlySnapshot::load(std::path::Path::new("/etc/ciel/manifest.json"))?;
/// let runtime = ReadOnlyRuntimeSnapshot::prepare(snapshot,
///     std::path::Path::new("/etc/ciel/gateway.json"))?;
/// let inputs = runtime.inputs()?; // Data only; does not authorize serving.
/// assert!(!inputs.owner.is_empty());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct ReadOnlyRuntimeSnapshot {
    snapshot: ReadOnlySnapshot,
    gateway_plan: PathBuf,
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    gateway_digest: String,
    secrets: RuntimeSecrets,
    budget: ReadBudgetPolicy,
}

/// Borrowed, validated data; copied values do not carry admission authority.
///
/// Secret references must never enter logs, responses, journals or model input.
/// Only the Core business token may be independently provisioned to Gateway.
///
/// # Examples
/// ```
/// use secretary_core::infrastructure::service_runtime::RuntimeInputs;
/// fn inspect(inputs: RuntimeInputs<'_>) {
///     assert!(!inputs.owner.is_empty());
///     assert_ne!(inputs.core_token, inputs.operator_token);
/// }
/// ```
pub struct RuntimeInputs<'a> {
    /// The once-read protected processing policy; no implicit quota defaults.
    pub budget_policy: &'a ReadBudgetPolicy,
    /// The original protected read-only registry; no environment replacement.
    pub registry: &'a Registry,
    /// Exact configured identity, still requiring trusted host-routing binding.
    pub owner: &'a str,
    /// Business bearer; never an operator or approval grant.
    pub core_token: &'a str,
    /// Distinct operator bearer; no approval/apply authority in this profile.
    pub operator_token: &'a str,
    /// Principal decoded from the same credential bytes as the signing key.
    pub google_principal: &'a str,
    /// Locally validated RSA signing material; no OAuth has been attempted.
    pub google_key: &'a EncodingKey,
    /// Administrator-selected generation, still requiring admission binding.
    pub deployment_generation: &'a str,
    /// Protected state directory; preparation does not open a journal.
    pub state_directory: &'a Path,
}

impl ReadOnlyRuntimeSnapshot {
    /// Consume bootstrap before creating runtime threads or resources.
    ///
    /// The resulting authority permits directory content changes, never input
    /// replacement or authority changes. Full admission is still required.
    ///
    /// # Errors
    /// Returns a sanitized bootstrap/platform denial before transitioning.
    pub fn into_runtime(mut self) -> Result<ReadOnlyRuntimeAuthority, &'static str> {
        self.check_pre_runtime()?;
        #[cfg(target_os = "linux")]
        let context = super::gateway_launch::RuntimeContext::capture(&self.snapshot)?;
        self.snapshot.files.enter_runtime()?;
        Ok(ReadOnlyRuntimeAuthority {
            prepared: self,
            #[cfg(target_os = "linux")]
            context,
            check_lock: Mutex::new(()),
            fenced: AtomicBool::new(false),
        })
    }
    /// Consume a protected snapshot in the actual single-threaded Core process.
    ///
    /// Check both processes before opening any secret body. Pin the exact Gateway
    /// plan/executable/intent in this snapshot; a later check cannot adopt another
    /// launch even if the administrator reused a generation. Budget is read once
    /// from its held inode and validated with the explicit runtime-only schema.
    /// No runtime object, listener, client or state file is created.
    ///
    /// # Errors
    /// Returns only static denial codes, never input values or parser errors.
    pub fn prepare(
        mut snapshot: ReadOnlySnapshot,
        gateway_plan: &Path,
    ) -> Result<Self, &'static str> {
        check_environment()?;
        snapshot.check_gateway_process(gateway_plan)?;
        #[cfg(target_os = "linux")]
        let _gateway_digest = {
            let (files, digest) =
                super::gateway_launch::retain_gateway_inputs(&snapshot, gateway_plan)?;
            snapshot.files.retain(files)?;
            digest
        };
        snapshot.check_gateway_process(gateway_plan)?;
        let manifest = &snapshot.checked.manifest;
        let owner = snapshot.files.read_private(&manifest.owner_file, 257)?;
        let core = snapshot
            .files
            .read_private(&manifest.core_token_file, 4097)?;
        let operator = snapshot
            .files
            .read_private(&manifest.operator_token_file, 4097)?;
        let credentials = snapshot
            .files
            .read_private(&manifest.google_credentials_file, 65_536)?;
        let secrets = RuntimeSecrets::decode(&owner, &core, &operator, &credentials)?;
        let budget: ReadBudgetPolicy = config::decode(
            &snapshot
                .files
                .read_private(&manifest.budget_policy_file, 65_536)?,
        )
        .map_err(|_| "budget_policy_invalid")?;
        budget.validate()?;
        let prepared = Self {
            snapshot,
            gateway_plan: gateway_plan.to_owned(),
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            gateway_digest: _gateway_digest,
            secrets,
            budget,
        };
        prepared.check_pre_runtime()?;
        Ok(prepared)
    }

    /// Recheck pinned file authority, environment and both actual processes.
    ///
    /// This remains a single-threaded, point-in-time pre-runtime check. It neither
    /// silently reloads changed inputs nor provides lifetime runtime supervision.
    ///
    /// # Errors
    /// Returns sanitized change, authority, environment or Gateway denial codes.
    pub fn check_pre_runtime(&self) -> Result<(), &'static str> {
        check_environment()?;
        self.snapshot.check_gateway_process(&self.gateway_plan)
    }

    /// Borrow the same in-memory configuration and secrets after revalidation.
    ///
    /// # Errors
    /// Returns a sanitized denial if pre-runtime authority no longer holds.
    pub fn inputs(&self) -> Result<RuntimeInputs<'_>, &'static str> {
        self.check_pre_runtime()?;
        self.borrowed_inputs()
    }

    fn borrowed_inputs(&self) -> Result<RuntimeInputs<'_>, &'static str> {
        let manifest = &self.snapshot.checked.manifest;
        Ok(RuntimeInputs {
            budget_policy: &self.budget,
            registry: self.snapshot.registry()?,
            owner: &self.secrets.owner,
            core_token: &self.secrets.core_token,
            operator_token: &self.secrets.operator_token,
            google_principal: &self.secrets.google_principal,
            google_key: &self.secrets.google_key,
            deployment_generation: &manifest.deployment_generation,
            state_directory: Path::new(&manifest.state_directory),
        })
    }
}

/// Explicit manifest startup with independently reviewed, protected evidence.
///
/// The six-argument legacy form remains denied. The admitted form requires the
/// installed plugin directory and protected promotion record. Status-only output
/// preserves unsafe inherited standard handles.
///
/// # Examples
/// ```
/// assert_eq!(secretary_core::infrastructure::service_runtime::start(&[]), 2);
/// ```
pub fn start(args: &[std::ffi::OsString]) -> i32 {
    if !matches!(args.len(), 6 | 10)
        || args[0] != "service"
        || args[1] != "start-readonly"
        || args[2] != "--manifest"
        || args[4] != "--gateway-plan"
    {
        return 2;
    }
    if args.len() == 6 {
        return 1;
    }
    if args[6] != "--plugin-dir" || args[8] != "--promotion" {
        return 2;
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        let result = (|| {
            let plugin = Path::new(&args[7]);
            let snapshot = admission::load_snapshot(Path::new(&args[3]), plugin)?;
            let prepared =
                admission::authorize(snapshot, Path::new(&args[5]), plugin, Path::new(&args[9]))?;
            let authority = std::sync::Arc::new(prepared.into_runtime()?);
            let runtime = new_runtime()?;
            runtime.block_on(async {
                authority.check_runtime()?;
                let listener = tokio::net::TcpListener::bind("127.0.0.1:8787")
                    .await
                    .map_err(|_| "runtime_listener_failed")?;
                let state = compose(authority.clone()).await?;
                authority.check_runtime()?;
                axum::serve(listener, crate::router(state))
                    .await
                    .map_err(|_| "runtime_serve_failed")
            })
        })();
        i32::from(result.is_err())
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        1
    }
}

/// Verify release compatibility in the actual protected Core process.
///
/// This command checks the selected release and installed plugin before secret
/// access. It returns status 0 on compatibility, 1 on denial, and 2 on bad usage.
///
/// # Examples
/// ```
/// assert_eq!(secretary_core::infrastructure::service_runtime::check_release(&[]), 2);
/// ```
pub fn check_release(args: &[std::ffi::OsString]) -> i32 {
    if args.len() != 6
        || args[0] != "service"
        || args[1] != "check-release-readonly"
        || args[2] != "--manifest"
        || args[4] != "--plugin-dir"
    {
        return 2;
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        i32::from(admission::load_snapshot(Path::new(&args[3]), Path::new(&args[5])).is_err())
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        1
    }
}

/// Verify a proposed promotion against current protected inputs without starting Core.
///
/// Uses the same arguments and binding checks as the admitted startup, with
/// the command name `review-readonly`. Returns 1 for a denied review.
///
/// # Examples
/// ```
/// assert_eq!(secretary_core::infrastructure::service_runtime::review(&[]), 2);
/// ```
pub fn review(args: &[std::ffi::OsString]) -> i32 {
    if args.len() != 10
        || args[0] != "service"
        || args[1] != "review-readonly"
        || args[2] != "--manifest"
        || args[4] != "--gateway-plan"
        || args[6] != "--plugin-dir"
        || args[8] != "--promotion"
    {
        return 2;
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        let plugin = Path::new(&args[7]);
        let result = admission::load_snapshot(Path::new(&args[3]), plugin).and_then(|snapshot| {
            admission::authorize(snapshot, Path::new(&args[5]), plugin, Path::new(&args[9]))
        });
        i32::from(result.is_err())
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        1
    }
}

/// Run the explicitly authorized qualification plan with a bounded private capture.
///
/// The protected plan and installed release are checked before loading secrets.
/// Status 2 denotes invalid syntax and status 1 denotes denial or a failed run.
///
/// # Examples
/// ```
/// assert_eq!(secretary_core::infrastructure::service_runtime::qualify(&[]), 2);
/// ```
pub fn qualify(args: &[std::ffi::OsString]) -> i32 {
    if args.len() != 14
        || args[0] != "service"
        || args[1] != "qualify-readonly"
        || args[2] != "--manifest"
        || args[4] != "--gateway-plan"
        || args[6] != "--plugin-dir"
        || args[8] != "--plan"
        || args[10] != "--output"
        || args[12] != "--supervisor-pid"
    {
        return 2;
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        i32::from(
            admission::qualification::run(
                Path::new(&args[3]),
                Path::new(&args[5]),
                Path::new(&args[7]),
                Path::new(&args[9]),
                Path::new(&args[11]),
                args[13]
                    .to_str()
                    .and_then(|value| value.parse::<i32>().ok())
                    .unwrap_or(0),
            )
            .is_err(),
        )
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        1
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn new_runtime() -> Result<tokio::runtime::Runtime, &'static str> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "runtime_initialization_failed")
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn readable_sources(registry: &Registry) -> (bool, bool) {
    (
        registry.spreadsheets.iter().any(|source| source.readable),
        registry.documents.iter().any(|source| source.readable),
    )
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn google_client(input: &RuntimeInputs<'_>) -> Result<super::google::GoogleClient, &'static str> {
    let (sheets, docs) = readable_sources(input.registry);
    super::google::GoogleClient::read_only_key(
        input.google_principal.to_owned(),
        input.google_key.clone(),
        sheets,
        docs,
    )
    .map_err(|_| "runtime_client_failed")
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
async fn compose(
    authority: std::sync::Arc<ReadOnlyRuntimeAuthority>,
) -> Result<crate::AppState, &'static str> {
    use super::journal::{read_budget::JournalReadTasks, Journal};
    let input = authority.inputs()?;
    let journal = Journal::open_read_only_bound(
        input.state_directory,
        input.owner,
        input.deployment_generation,
    )
    .await
    .map_err(|_| "runtime_journal_failed")?;
    let ledger = JournalReadTasks::open(
        journal.clone(),
        input.owner,
        input.deployment_generation,
        input.budget_policy.clone(),
    )
    .await
    .map_err(|_| "runtime_budget_failed")?;
    authority.check_runtime()?;
    let google = google_client(&input)?;
    let mut state = crate::AppState::new_read_only(
        input.registry.clone(),
        input.core_token.to_owned(),
        input.owner.to_owned(),
        google,
    )
    .map_err(|_| "runtime_state_failed")?
    .with_writes(
        crate::writes::Writes::new(journal, false),
        input.operator_token.to_owned(),
    )
    .map_err(|_| "runtime_state_failed")?
    .with_read_tasks(ledger, input.operator_token.to_owned())
    .map_err(|_| "runtime_state_failed")?;
    state.bus = state
        .bus
        .with_runtime_authority(authority.clone())
        .map_err(|_| "runtime_state_failed")?;
    authority.check_runtime()?;
    Ok(state)
}

struct RuntimeSecrets {
    owner: String,
    core_token: String,
    operator_token: String,
    google_principal: String,
    google_key: EncodingKey,
}

impl RuntimeSecrets {
    fn decode(
        owner: &[u8],
        core: &[u8],
        operator: &[u8],
        credentials: &[u8],
    ) -> Result<Self, &'static str> {
        let owner = text(owner, 256).ok_or("runtime_owner_invalid")?;
        let core_token = token(core).ok_or("runtime_token_invalid")?;
        let operator_token = token(operator).ok_or("runtime_token_invalid")?;
        if core_token == operator_token {
            return Err("runtime_tokens_not_distinct");
        }
        if credentials.len() > 65_536 {
            return Err("runtime_secret_too_large");
        }
        let account: Credential =
            config::decode(credentials).map_err(|_| "runtime_credentials_invalid")?;
        let google_key = account.validate()?;
        Ok(Self {
            owner,
            core_token,
            operator_token,
            google_principal: account.client_email,
            google_key,
        })
    }
}

fn text(bytes: &[u8], max: usize) -> Option<String> {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let value = std::str::from_utf8(bytes).ok()?;
    (!value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control))
        .then(|| value.to_owned())
}

fn token(bytes: &[u8]) -> Option<String> {
    let value = text(bytes, 4096)?;
    let body = value.trim_end_matches('=');
    let lower = value.to_ascii_lowercase();
    (value.len() >= 32
        && !body.is_empty()
        && body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/".contains(&byte))
        && body.bytes().any(|byte| byte != body.as_bytes()[0])
        && ![
            "replace",
            "change-me",
            "changeme",
            "example",
            "placeholder",
            "your-token",
            "insert-token",
        ]
        .iter()
        .any(|marker| lower.contains(marker)))
    .then_some(value)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Credential {
    #[serde(rename = "type")]
    kind: String,
    client_email: String,
    private_key: String,
    token_uri: String,
    universe_domain: Option<String>,
    auth_uri: Option<String>,
    #[serde(rename = "project_id")]
    _project_id: Option<String>,
    #[serde(rename = "private_key_id")]
    _private_key_id: Option<String>,
    #[serde(rename = "client_id")]
    _client_id: Option<String>,
    #[serde(rename = "auth_provider_x509_cert_url")]
    _auth_provider_x509_cert_url: Option<String>,
    #[serde(rename = "client_x509_cert_url")]
    _client_x509_cert_url: Option<String>,
}

impl Credential {
    fn validate(&self) -> Result<EncodingKey, &'static str> {
        if self.kind != "service_account"
            || !service_email(&self.client_email)
            || self.token_uri != "https://oauth2.googleapis.com/token"
            || self
                .universe_domain
                .as_deref()
                .is_some_and(|domain| domain != "googleapis.com")
            || self
                .auth_uri
                .as_deref()
                .is_some_and(|uri| uri != "https://accounts.google.com/o/oauth2/auth")
        {
            return Err("runtime_credentials_invalid");
        }
        let key = EncodingKey::from_rsa_pem(self.private_key.as_bytes())
            .map_err(|_| "runtime_credentials_invalid")?;
        // PEM decoding alone need not prove a usable RSA private key. Sign only
        // a local non-OAuth payload, discard the result, and never contact Google.
        jsonwebtoken::encode(
            &Header::new(Algorithm::RS256),
            &serde_json::json!({"purpose":"ciel-local-key-validation.v1"}),
            &key,
        )
        .map_err(|_| "runtime_credentials_invalid")?;
        Ok(key)
    }
}

fn service_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    let Some(project) = domain.strip_suffix(".iam.gserviceaccount.com") else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !project.is_empty()
        && value.len() <= 254
        && [local, project].iter().all(|part| {
            part.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-._".contains(&byte)
            })
        })
}

fn check_environment() -> Result<(), &'static str> {
    if std::env::vars_os().any(|(key, _)| environment_conflicts(&key.to_string_lossy())) {
        return Err("service_environment_conflict");
    }
    Ok(())
}

fn environment_conflicts(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    upper.starts_with("SECRETARY_")
        || upper == "GOOGLE_APPLICATION_CREDENTIALS"
        || upper.starts_with("LD_")
        || matches!(upper.as_str(), "GLIBC_TUNABLES" | "GCONV_PATH")
        || matches!(
            upper.as_str(),
            "HTTP_PROXY" | "HTTPS_PROXY" | "ALL_PROXY" | "NO_PROXY"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const CORE: &[u8] = b"SYNTHETIC-CORE-0123456789-abcdefgh";
    const OPERATOR: &[u8] = b"SYNTHETIC-OPERATOR-9876543210-zyxw";

    fn credential() -> Value {
        json!({"type":"service_account", "client_email":"synthetic@test-project.iam.gserviceaccount.com",
            "private_key":include_str!("../../tests/support/synthetic-rsa.pem"),
            "token_uri":"https://oauth2.googleapis.com/token", "universe_domain":"googleapis.com"})
    }

    fn decode(value: &Value) -> Result<RuntimeSecrets, &'static str> {
        RuntimeSecrets::decode(
            b" exact synthetic owner \n",
            CORE,
            OPERATOR,
            &serde_json::to_vec(value).unwrap(),
        )
    }

    #[test]
    fn credentials_and_text_are_validated_without_normalizing_identity() {
        let result = decode(&credential()).ok().unwrap();
        assert_eq!(result.owner, " exact synthetic owner ");
        assert_eq!(result.core_token.as_bytes(), CORE);
        assert_eq!(result.operator_token.as_bytes(), OPERATOR);
        assert_eq!(
            result.google_principal,
            "synthetic@test-project.iam.gserviceaccount.com"
        );
        assert_eq!(text("é".repeat(128).as_bytes(), 256).unwrap().len(), 256);
        assert!(text("é".repeat(129).as_bytes(), 256).is_none());
    }

    #[test]
    fn text_rejects_extra_records_controls_empty_invalid_utf8_and_oversize() {
        for value in [
            b"".as_slice(),
            b"\n",
            b" \n",
            b"owner\n\n",
            b"owner\r\n",
            b"owner\0",
            b"owner\t",
            b"\xff",
        ] {
            assert!(text(value, 256).is_none());
        }
        assert!(text(&[b'x'; 257], 256).is_none());
        assert_eq!(text(b"owner\n", 256).as_deref(), Some("owner"));
    }

    #[test]
    fn tokens_reject_short_placeholder_repeated_and_non_bearer_values() {
        for value in [
            "short".to_owned(),
            "x".repeat(32),
            "=".repeat(32),
            "replace-with-a-random-token-at-least-32-bytes".to_owned(),
            "PLACEHOLDER-TOKEN-0123456789-abcdef".to_owned(),
            format!(" {}", String::from_utf8_lossy(CORE)),
            format!("{}=broken", String::from_utf8_lossy(CORE)),
            "é".repeat(32),
            "Xy".repeat(2049),
        ] {
            assert!(token(value.as_bytes()).is_none());
        }
        assert!(token(&[CORE, b"==\n"].concat()).is_some());
        let bytes = serde_json::to_vec(&credential()).unwrap();
        assert_eq!(
            RuntimeSecrets::decode(b"owner", CORE, CORE, &bytes).err(),
            Some("runtime_tokens_not_distinct")
        );
    }

    #[test]
    fn credential_shape_principal_endpoints_and_actual_rsa_key_are_required() {
        for (field, value) in [
            ("type", "authorized_user"),
            ("client_email", "owner@example.com"),
            ("client_email", "synthetic@.iam.gserviceaccount.com"),
            ("token_uri", "http://127.0.0.1/token"),
            ("universe_domain", "example.org"),
            ("auth_uri", "https://example.org/authorize"),
            ("private_key", "not-a-private-key"),
            (
                "private_key",
                "-----BEGIN PRIVATE KEY-----\nAQID\n-----END PRIVATE KEY-----",
            ),
            ("unexpected", "secret"),
        ] {
            let mut candidate = credential();
            candidate[field] = json!(value);
            assert_eq!(
                decode(&candidate).err(),
                Some("runtime_credentials_invalid")
            );
        }
        for field in ["type", "client_email", "private_key", "token_uri"] {
            let mut candidate = credential();
            candidate.as_object_mut().unwrap().remove(field);
            assert_eq!(
                decode(&candidate).err(),
                Some("runtime_credentials_invalid")
            );
        }
    }

    #[test]
    fn duplicate_json_keys_and_large_credentials_have_sanitized_errors() {
        let bytes = serde_json::to_vec(&credential()).unwrap();
        let duplicate = [b"{\"type\":\"PRIVATE_CANARY\",".as_slice(), &bytes[1..]].concat();
        assert_eq!(
            RuntimeSecrets::decode(b"owner", CORE, OPERATOR, &duplicate).err(),
            Some("runtime_credentials_invalid")
        );
        assert_eq!(
            RuntimeSecrets::decode(b"owner", CORE, OPERATOR, &[b' '; 65_537]).err(),
            Some("runtime_secret_too_large")
        );
    }

    #[test]
    fn competing_environment_names_are_rejected_without_value_inspection() {
        for name in [
            "SECRETARY_CORE_TOKEN",
            "secretary_config",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "http_proxy",
            "HTTPS_PROXY",
            "All_Proxy",
            "no_proxy",
        ] {
            assert!(environment_conflicts(name));
        }
        for name in ["HOME", "PATH", "LANG", "LC_ALL"] {
            assert!(!environment_conflicts(name));
        }
    }
}
