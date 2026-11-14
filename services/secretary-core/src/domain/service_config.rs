//! Strict offline service configuration. These values confer no runtime authority.
use serde::Deserialize;
use std::collections::BTreeSet;

/// Versioned read-only manifest; file authority is checked separately by the loader.
///
/// # Examples
///
/// ```
/// use secretary_core::domain::service_config::Manifest;
/// fn validate(input: &str) -> Result<(), Box<dyn std::error::Error>> {
///     let manifest: Manifest = serde_json::from_str(input)?;
///     manifest.validate()?;
///     Ok(())
/// }
/// ```
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub(crate) schema: String,
    pub(crate) profile: String,
    pub(crate) deployment_generation: String,
    pub(crate) owner_file: String,
    pub(crate) registry_file: String,
    pub(crate) core_token_file: String,
    pub(crate) operator_token_file: String,
    pub(crate) google_credentials_file: String,
    pub(crate) state_directory: String,
    pub(crate) release_file: String,
    pub(crate) service_policy_file: String,
    pub(crate) budget_policy_file: String,
}

impl Manifest {
    /// Validate the manifest shape without reading any referenced file.
    ///
    /// # Errors
    /// Returns a sanitized code for invalid versions, paths or conflicting inputs.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != "ciel.readonly.manifest.v1" || self.profile != "readonly" {
            return Err("manifest_version_or_profile");
        }
        if !revision(&self.deployment_generation) {
            return Err("manifest_generation");
        }
        let paths = self.paths();
        if paths.iter().any(|path| !absolute_path(path)) {
            return Err("manifest_path");
        }
        if paths.iter().collect::<BTreeSet<_>>().len() != paths.len() {
            return Err("manifest_path_alias");
        }
        let prefix = format!("{}/", self.state_directory);
        if self.files().iter().any(|path| path.starts_with(&prefix)) {
            return Err("manifest_state_contains_input");
        }
        Ok(())
    }

    pub(crate) fn files(&self) -> [&str; 8] {
        [
            &self.owner_file,
            &self.registry_file,
            &self.core_token_file,
            &self.operator_token_file,
            &self.google_credentials_file,
            &self.release_file,
            &self.service_policy_file,
            &self.budget_policy_file,
        ]
    }

    fn paths(&self) -> Vec<&str> {
        self.files()
            .into_iter()
            .chain([self.state_directory.as_str()])
            .collect()
    }
}

/// Numeric local account selected by administrator-controlled policy.
/// Deserializing this value alone does not validate an account or grant access.
///
/// # Examples
///
/// ```
/// use secretary_core::domain::service_config::Principal;
/// let principal: Principal = serde_json::from_str(r#"{"uid":10001,"gid":10001}"#)?;
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Principal {
    pub(crate) uid: u32,
    pub(crate) gid: u32,
}

/// Initial service policy admits one fixed Linux launcher/filesystem contract.
///
/// # Examples
///
/// ```
/// use secretary_core::domain::service_config::ServicePolicy;
/// fn validate(input: &str) -> Result<(), Box<dyn std::error::Error>> {
///     let policy: ServicePolicy = serde_json::from_str(input)?;
///     policy.validate()?;
///     Ok(())
/// }
/// ```
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServicePolicy {
    pub(crate) schema: String,
    pub(crate) revision: String,
    pub(crate) core: Principal,
    pub(crate) gateway: Principal,
    pub(crate) listener: String,
    pub(crate) filesystem: String,
    pub(crate) launcher: String,
    pub(crate) endpoints: String,
}

impl ServicePolicy {
    /// Validate policy values; actual local accounts and filesystem remain unchecked.
    ///
    /// # Errors
    /// Returns a sanitized code for unsupported policy or overlapping identities.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != "ciel.readonly.service-policy.v1"
            || self.filesystem != "linux-root-owned-no-acl.v1"
            || self.launcher != "ciel.readonly.launcher.v1"
            || self.endpoints != "google-production.v1"
            || self.listener != "127.0.0.1:8787"
            || !revision(&self.revision)
        {
            return Err("service_policy_unsupported");
        }
        if self.core.uid == 0
            || self.core.gid == 0
            || self.gateway.uid == 0
            || self.gateway.gid == 0
            || self.core.uid == self.gateway.uid
            || self.core.gid == self.gateway.gid
        {
            return Err("service_identity_conflict");
        }
        Ok(())
    }
}

pub(crate) fn revision(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

pub(crate) fn absolute_path(value: &str) -> bool {
    value.len() >= 2
        && value.len() <= 4096
        && value.starts_with('/')
        && value[1..].split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
}
