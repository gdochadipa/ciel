//! Read the current thread's kernel authority; never launch or modify a process.
use crate::domain::service_config::ServicePolicy;

/// Check the current single-threaded process against protected service policy.
///
/// Call before creating worker threads, opening runtime secrets or starting Core.
/// The caller must obtain policy from the administrator-controlled effective
/// configuration. This check neither authenticates a caller-supplied policy nor
/// qualifies Gateway isolation, a service manager, namespace authority, future
/// process state or service admission. Legacy startup does not call this guard.
/// Unknown inherited handles are rejected; stdio must use null/one-way anonymous
/// pipes. Snapshot-owned read/metadata handles require the snapshot-bound method.
/// A startup caller must not log a denial into unvalidated inherited stdio.
///
/// # Examples
///
/// ```no_run
/// use secretary_core::infrastructure::service_check::ReadOnlySnapshot;
/// let snapshot = ReadOnlySnapshot::load(std::path::Path::new("/etc/secretary/readonly/manifest.json"))?;
/// snapshot.check_current_process()?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
/// Returns sanitized codes on unsupported platforms, unavailable/inconsistent
/// kernel observations, invalid policy or excess process authority.
pub fn check_current_process(policy: &ServicePolicy) -> Result<(), &'static str> {
    policy.validate()?;
    check(policy)
}

pub(super) fn check_snapshot_process(
    policy: &ServicePolicy,
    files: &super::service_files::SafeFiles,
) -> Result<(), &'static str> {
    policy.validate()?;
    #[cfg(target_os = "linux")]
    {
        check_with_descriptors(policy, &files.descriptor_numbers())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = files;
        check(policy)
    }
}

#[cfg(not(target_os = "linux"))]
fn check(_: &ServicePolicy) -> Result<(), &'static str> {
    Err("launcher_authority_unsupported")
}

#[cfg(target_os = "linux")]
fn check(policy: &ServicePolicy) -> Result<(), &'static str> {
    check_with_descriptors(policy, &std::collections::BTreeSet::new())
}

#[cfg(target_os = "linux")]
fn check_with_descriptors(
    policy: &ServicePolicy,
    known_inputs: &std::collections::BTreeSet<i32>,
) -> Result<(), &'static str> {
    check_principal(policy.core, known_inputs)
}

#[cfg(target_os = "linux")]
pub(super) fn check_principal(
    principal: crate::domain::service_config::Principal,
    known_inputs: &std::collections::BTreeSet<i32>,
) -> Result<(), &'static str> {
    let before = linux::observe()?;
    before.validate(principal)?;
    let descriptors = super::launch_descriptors::observe(known_inputs)?;
    crate::domain::launch_authority::validate_descriptors(&descriptors)?;
    if linux::observe()? != before
        || super::launch_descriptors::observe(known_inputs)? != descriptors
    {
        return Err("launcher_authority_changed");
    }
    Ok(())
}

// Keep the strict kernel decoder testable on other development platforms too.
#[cfg(any(target_os = "linux", test))]
pub(super) mod status {
    use crate::domain::launch_authority::LaunchAuthority;
    use std::collections::BTreeMap;

    pub(in crate::infrastructure) fn parse(bytes: &[u8]) -> Result<LaunchAuthority, &'static str> {
        let text = std::str::from_utf8(bytes).map_err(|_| "launcher_status_invalid")?;
        let names = [
            "Uid",
            "Gid",
            "Groups",
            "CapInh",
            "CapPrm",
            "CapEff",
            "CapBnd",
            "CapAmb",
            "NoNewPrivs",
            "Threads",
        ];
        let mut fields = BTreeMap::new();
        for (name, value) in text.lines().filter_map(|line| line.split_once(':')) {
            if names.contains(&name) && fields.insert(name, value.trim()).is_some() {
                return Err("launcher_status_invalid");
            }
        }
        if fields.len() != names.len() {
            return Err("launcher_status_invalid");
        }
        let capabilities: Result<Vec<_>, _> = ["CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb"]
            .into_iter()
            .map(|name| {
                let value = fields[name];
                if value.len() != 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err("launcher_status_invalid");
                }
                u64::from_str_radix(value, 16).map_err(|_| "launcher_status_invalid")
            })
            .collect();
        Ok(LaunchAuthority {
            uids: numbers(fields["Uid"])?
                .try_into()
                .map_err(|_| "launcher_status_invalid")?,
            gids: numbers(fields["Gid"])?
                .try_into()
                .map_err(|_| "launcher_status_invalid")?,
            groups: numbers(fields["Groups"])?,
            capabilities: capabilities?
                .try_into()
                .map_err(|_| "launcher_status_invalid")?,
            no_new_privileges: match fields["NoNewPrivs"] {
                "0" => false,
                "1" => true,
                _ => return Err("launcher_status_invalid"),
            },
            threads: number(fields["Threads"])?,
        })
    }

    fn numbers(value: &str) -> Result<Vec<u32>, &'static str> {
        value.split_ascii_whitespace().map(number).collect()
    }

    fn number(value: &str) -> Result<u32, &'static str> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("launcher_status_invalid");
        }
        value.parse().map_err(|_| "launcher_status_invalid")
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::status;
    use crate::domain::launch_authority::LaunchAuthority;
    use std::{fs::File, io::Read, os::fd::AsRawFd};

    pub(super) fn observe() -> Result<LaunchAuthority, &'static str> {
        // thread-self selects this calling thread even if a future caller starts
        // workers too early. The domain guard then rejects a multi-thread process.
        let file =
            File::open("/proc/thread-self/status").map_err(|_| "launcher_authority_unavailable")?;
        let mut filesystem = std::mem::MaybeUninit::<libc::statfs>::uninit();
        // fstatfs initializes the output only on success; the descriptor stays held.
        if unsafe { libc::fstatfs(file.as_raw_fd(), filesystem.as_mut_ptr()) } != 0 {
            return Err("launcher_authority_unavailable");
        }
        // Never interpret an ordinary file mounted over the procfs status path.
        let filesystem = unsafe { filesystem.assume_init() };
        if filesystem.f_type != libc::PROC_SUPER_MAGIC as libc::c_long {
            return Err("launcher_authority_unavailable");
        }
        let mut bytes = Vec::new();
        file.take(65_537)
            .read_to_end(&mut bytes)
            .map_err(|_| "launcher_authority_unavailable")?;
        if bytes.len() > 65_536 {
            return Err("launcher_status_invalid");
        }
        status::parse(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::status;

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn unsupported_platforms_cannot_attest_authority_or_bypass_policy_validation() {
        let mut policy: super::ServicePolicy = serde_json::from_str(include_str!(
            "../../../../config/service-policy.example.json"
        ))
        .unwrap();
        assert_eq!(
            super::check_current_process(&policy),
            Err("launcher_authority_unsupported")
        );
        policy.core.uid = 0;
        assert_eq!(
            super::check_current_process(&policy),
            Err("service_identity_conflict")
        );
    }

    const STATUS: &str = "Name:\tfixture\nUid:\t10001 10001 10001 10001\nGid:\t10001 10001 10001 10001\nGroups:\t10001\nCapInh:\t0000000000000000\nCapPrm:\t0000000000000000\nCapEff:\t0000000000000000\nCapBnd:\t0000000000000000\nCapAmb:\t0000000000000000\nNoNewPrivs:\t1\nThreads:\t1\n";

    #[test]
    fn kernel_status_requires_every_authority_field_once_and_exact_numeric_shapes() {
        let observed = status::parse(STATUS.as_bytes()).unwrap();
        assert_eq!(observed.uids, [10001; 4]);
        assert_eq!(observed.groups, [10001]);
        assert!(observed.no_new_privileges);
        for line in STATUS.lines().filter(|line| !line.starts_with("Name:")) {
            let missing = STATUS.replace(&format!("{line}\n"), "");
            assert_eq!(
                status::parse(missing.as_bytes()).unwrap_err(),
                "launcher_status_invalid"
            );
            let duplicate = format!("{STATUS}{line}\n");
            assert_eq!(
                status::parse(duplicate.as_bytes()).unwrap_err(),
                "launcher_status_invalid"
            );
        }
        for (before, after) in [
            ("Uid:\t10001 10001 10001 10001", "Uid:\t10001 10001 10001"),
            (
                "Gid:\t10001 10001 10001 10001",
                "Gid:\t10001 10001 10001 4294967296",
            ),
            ("Groups:\t10001", "Groups:\t-1"),
            ("CapEff:\t0000000000000000", "CapEff:\txyz"),
            ("NoNewPrivs:\t1", "NoNewPrivs:\t2"),
            ("Threads:\t1", "Threads:\t1 2"),
        ] {
            assert_eq!(
                status::parse(STATUS.replace(before, after).as_bytes()).unwrap_err(),
                "launcher_status_invalid"
            );
        }
        assert!(status::parse(&[0xff]).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn actual_kernel_observation_matches_current_thread_ids_and_groups() {
        let observed = super::linux::observe().unwrap();
        // These independent syscalls verify the procfs adapter's real/effective
        // identity and group decoding without changing credentials or privileges.
        assert_eq!(observed.uids[0], unsafe { libc::getuid() });
        assert_eq!(observed.uids[1], unsafe { libc::geteuid() });
        assert_eq!(observed.gids[0], unsafe { libc::getgid() });
        assert_eq!(observed.gids[1], unsafe { libc::getegid() });
        let count = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
        assert!(count >= 0);
        let mut groups = vec![0; count as usize];
        assert_eq!(
            unsafe { libc::getgroups(count, groups.as_mut_ptr()) },
            count
        );
        assert_eq!(observed.groups, groups);
        let no_new_privileges = unsafe { libc::prctl(libc::PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0) };
        assert!(matches!(no_new_privileges, 0 | 1));
        assert_eq!(observed.no_new_privileges, no_new_privileges == 1);
    }
}
