//! Administrator-selected exec input; an intent is never proof of host readiness.
use super::service_config::{absolute_path, revision};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GatewayPlan {
    pub schema: String,
    pub deployment_generation: String,
    pub manifest_sha256: String,
    pub policy_sha256: String,
    pub executable: String,
    pub executable_sha256: String,
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub working_directory: String,
    pub intent_file: String,
}

impl GatewayPlan {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != "ciel.readonly.gateway-plan.v1"
            || !revision(&self.deployment_generation)
            || ![
                &self.manifest_sha256,
                &self.policy_sha256,
                &self.executable_sha256,
            ]
            .into_iter()
            .all(|value| digest(value))
            || ![&self.executable, &self.working_directory, &self.intent_file]
                .into_iter()
                .all(|path| absolute_path(path))
        {
            return Err("gateway_plan_invalid");
        }
        let environment_keys = [
            "HOME",
            "PATH",
            "LANG",
            "LC_ALL",
            "OPENCLAW_CONFIG_PATH",
            "OPENCLAW_STATE_DIR",
        ];
        if self.arguments.len() > 64
            || self
                .environment
                .keys()
                .any(|key| !environment_keys.contains(&key.as_str()))
            || self
                .arguments
                .iter()
                .chain(self.environment.values())
                .any(|value| value.len() > 8192 || value.as_bytes().contains(&0))
            || self.arguments.iter().map(String::len).sum::<usize>()
                + self
                    .environment
                    .iter()
                    .map(|(key, value)| key.len() + value.len())
                    .sum::<usize>()
                > 16384
        {
            return Err("gateway_exec_input_invalid");
        }
        Ok(())
    }
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) const NAMESPACES: [&str; 10] = [
    "cgroup",
    "ipc",
    "mnt",
    "net",
    "pid",
    "pid_for_children",
    "time",
    "time_for_children",
    "user",
    "uts",
];

pub(crate) fn validate_dump_mode(mode: u32) -> Result<(), &'static str> {
    // UID/GID changes reset dumpability to this sysctl. Mode 1 would allow a
    // same-UID peer to trace the transient process before its exec wipes memory.
    match mode {
        0 | 2 => Ok(()),
        _ => Err("gateway_dump_policy"),
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct NamespaceId {
    pub device: u64,
    pub inode: u64,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct NamespaceContext {
    pub namespaces: BTreeMap<String, NamespaceId>,
    pub uid_map: Vec<[u32; 3]>,
    pub gid_map: Vec<[u32; 3]>,
}

impl NamespaceContext {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.namespaces.len() != NAMESPACES.len()
            || NAMESPACES
                .iter()
                .any(|name| !self.namespaces.contains_key(*name))
            || self
                .namespaces
                .values()
                .any(|ns| ns.device == 0 || ns.inode == 0)
            || !valid_map(&self.uid_map)
            || !valid_map(&self.gid_map)
        {
            return Err("gateway_namespace_invalid");
        }
        Ok(())
    }
}

fn valid_map(rows: &[[u32; 3]]) -> bool {
    if rows.is_empty() || rows.len() > 340 {
        return false;
    }
    for (index, row) in rows.iter().enumerate() {
        if row[2] == 0
            || row[0].checked_add(row[2]).is_none()
            || row[1].checked_add(row[2]).is_none()
        {
            return false;
        }
        for earlier in &rows[..index] {
            if (0..2).any(|column| {
                row[column] < earlier[column] + earlier[2] && earlier[column] < row[column] + row[2]
            }) {
                return false;
            }
        }
    }
    true
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GatewayIntent {
    pub schema: String,
    pub deployment_generation: String,
    pub manifest_sha256: String,
    pub policy_sha256: String,
    pub plan_sha256: String,
    pub executable_sha256: String,
    pub pid: u32,
    pub start_ticks: u64,
    pub boot_id: String,
    pub context: NamespaceContext,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> GatewayPlan {
        GatewayPlan {
            schema: "ciel.readonly.gateway-plan.v1".into(),
            deployment_generation: "local-1".into(),
            manifest_sha256: "a".repeat(64),
            policy_sha256: "b".repeat(64),
            executable: "/opt/ciel/node".into(),
            executable_sha256: "c".repeat(64),
            arguments: vec!["/opt/ciel/gateway.js".into()],
            environment: BTreeMap::from([("HOME".into(), "/home/gateway".into())]),
            working_directory: "/home/gateway".into(),
            intent_file: "/run/ciel/intent.json".into(),
        }
    }

    #[test]
    fn exec_plan_is_explicit_bounded_and_cannot_inherit_loader_or_core_environment() {
        assert_eq!(plan().validate(), Ok(()));
        for key in [
            "LD_PRELOAD",
            "NODE_OPTIONS",
            "SECRETARY_CORE_TOKEN",
            "HTTPS_PROXY",
        ] {
            let mut value = plan();
            value.environment.insert(key.into(), "injected".into());
            assert_eq!(value.validate(), Err("gateway_exec_input_invalid"));
        }
        for argument in ["bad\0argument".into(), "x".repeat(8193)] {
            let mut value = plan();
            value.arguments = vec![argument];
            assert_eq!(value.validate(), Err("gateway_exec_input_invalid"));
        }
        let mut value = plan();
        value.executable = "/opt/../node".into();
        assert_eq!(value.validate(), Err("gateway_plan_invalid"));
        let mut value = plan();
        value.arguments = vec!["x".into(); 65];
        assert_eq!(value.validate(), Err("gateway_exec_input_invalid"));
    }

    #[test]
    fn namespace_maps_reject_empty_zero_overflow_and_overlapping_ranges() {
        assert!(valid_map(&[[0, 0, u32::MAX]]));
        assert!(valid_map(&[[0, 10000, 10], [100, 20000, 50]]));
        for rows in [
            vec![],
            vec![[0, 0, 0]],
            vec![[u32::MAX, 0, 1]],
            vec![[0, 0, 10], [5, 100, 10]],
            vec![[0, 0, 10], [100, 5, 10]],
        ] {
            assert!(!valid_map(&rows));
        }
    }

    #[test]
    fn credential_drop_cannot_make_transient_root_memory_dumpable_to_gateway_peers() {
        assert_eq!(validate_dump_mode(0), Ok(()));
        assert_eq!(validate_dump_mode(2), Ok(()));
        for mode in [1, 3, u32::MAX] {
            assert_eq!(validate_dump_mode(mode), Err("gateway_dump_policy"));
        }
    }
}
