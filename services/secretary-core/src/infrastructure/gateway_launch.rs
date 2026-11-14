//! Transient root-to-Gateway exec and protected intent verification; no daemon.
use super::{
    config,
    service_check::{FileKind, Files, ReadOnlySnapshot},
    service_files::SafeFiles,
    service_launcher,
};
use crate::domain::{
    gateway_launch::{
        validate_dump_mode, GatewayIntent, GatewayPlan, NamespaceContext, NamespaceId, NAMESPACES,
    },
    launch_authority::validate_descriptors,
    service_config::{absolute_path, Principal},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
    ffi::CString,
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{FileExt, MetadataExt},
    },
    path::Path,
};

struct ProtectedPlan {
    plan: GatewayPlan,
    digest: String,
    files: SafeFiles,
}

impl ProtectedPlan {
    fn load(snapshot: &ReadOnlySnapshot, path: &Path) -> Result<Self, &'static str> {
        let path = path
            .to_str()
            .filter(|value| absolute_path(value))
            .ok_or("gateway_plan_invalid")?;
        let (generation, manifest, policy_digest, policy) = snapshot.gateway_binding()?;
        let mut files = SafeFiles::new();
        let info = files.open(path, FileKind::Read)?;
        if info.gid != policy.core.gid || !matches!(info.mode, 0o440 | 0o640) {
            return Err("gateway_plan_authority");
        }
        let bytes = files.read(path)?;
        let plan: GatewayPlan = config::decode(&bytes).map_err(|_| "gateway_plan_invalid")?;
        plan.validate()?;
        if plan.deployment_generation != generation
            || plan.manifest_sha256 != manifest
            || plan.policy_sha256 != policy_digest
        {
            return Err("gateway_plan_binding");
        }
        let info = files.open(&plan.executable, FileKind::Read)?;
        if info.size > 128 * 1024 * 1024 {
            return Err("gateway_executable_too_large");
        }
        if info.mode & 0o111 != 0o111 || files.hash(&plan.executable)? != plan.executable_sha256 {
            return Err("gateway_executable_mismatch");
        }
        let mut magic = [0; 4];
        if files
            .file(&plan.executable)?
            .read_at(&mut magic, 0)
            .map_err(|_| "gateway_executable_mismatch")?
            != 4
            || magic != *b"\x7fELF"
        {
            // A script interpreter needs a surviving descriptor with AT_EMPTY_PATH.
            // Only ELF avoids leaking that handle into the Gateway execution.
            return Err("gateway_executable_unsupported");
        }
        files.confirm()?;
        Ok(Self {
            plan,
            digest: format!("{:x}", Sha256::digest(&bytes)),
            files,
        })
    }
}

pub(crate) fn exec_gateway(manifest: &Path, path: &Path) -> Result<Infallible, &'static str> {
    root_entry()?;
    let snapshot = ReadOnlySnapshot::load(manifest)?;
    let protected = ProtectedPlan::load(&snapshot, path)?;
    let (_, _, _, policy) = snapshot.gateway_binding()?;
    let principal = policy.gateway;
    let executable = protected
        .files
        .file(&protected.plan.executable)?
        .try_clone()
        .map_err(|_| "gateway_executable_mismatch")?;
    let arguments: Result<Vec<_>, _> = std::iter::once(&protected.plan.executable)
        .chain(protected.plan.arguments.iter())
        .map(|value| CString::new(value.as_bytes()))
        .collect();
    let environment: Result<Vec<_>, _> = protected
        .plan
        .environment
        .iter()
        .map(|(key, value)| CString::new(format!("{key}={value}")))
        .collect();
    let arguments = arguments.map_err(|_| "gateway_exec_input_invalid")?;
    let environment = environment.map_err(|_| "gateway_exec_input_invalid")?;
    let directory = CString::new(protected.plan.working_directory.as_bytes())
        .map_err(|_| "gateway_plan_invalid")?;
    let pid = std::process::id();
    let intent = GatewayIntent {
        schema: "ciel.readonly.gateway-intent.v1".into(),
        deployment_generation: protected.plan.deployment_generation.clone(),
        manifest_sha256: protected.plan.manifest_sha256.clone(),
        policy_sha256: protected.plan.policy_sha256.clone(),
        plan_sha256: protected.digest.clone(),
        executable_sha256: protected.plan.executable_sha256.clone(),
        pid,
        start_ticks: start_ticks(pid)?,
        boot_id: boot_id()?,
        context: own_context()?,
    };
    protected.files.confirm()?;
    snapshot.gateway_binding()?;
    publish(&protected.plan.intent_file, policy.core.gid, &intent)?;
    protected.files.confirm()?;
    snapshot.gateway_binding()?;
    // No protected input/record or ancestor handle may survive the credential
    // drop. Only this exact read-only, CLOEXEC executable is temporarily retained.
    drop(protected);
    drop(snapshot);
    if unsafe { libc::chdir(c"/".as_ptr()) } != 0 {
        return Err("gateway_working_directory");
    }
    drop_authority(principal)?;
    service_launcher::check_principal(principal, &BTreeSet::from([executable.as_raw_fd()]))?;
    if own_context()? != intent.context {
        return Err("gateway_namespace_mismatch");
    }
    if unsafe { libc::chdir(directory.as_ptr()) } != 0 {
        return Err("gateway_working_directory");
    }
    let argv: Vec<_> = arguments
        .iter()
        .map(|value| value.as_ptr())
        .chain([std::ptr::null()])
        .collect();
    let envp: Vec<_> = environment
        .iter()
        .map(|value| value.as_ptr())
        .chain([std::ptr::null()])
        .collect();
    // Execute the held inode, never resolve a mutable path after dropping UID.
    // The descriptor is CLOEXEC; a successful ELF exec closes the last input.
    unsafe {
        libc::syscall(
            libc::SYS_execveat,
            executable.as_raw_fd(),
            c"".as_ptr(),
            argv.as_ptr(),
            envp.as_ptr(),
            libc::AT_EMPTY_PATH,
        );
    }
    Err("gateway_exec_failed")
}

fn root_entry() -> Result<(), &'static str> {
    let authority =
        service_launcher::status::parse(&read_proc("/proc/thread-self/status", 65536)?)?;
    if authority.uids != [0; 4] || authority.gids != [0; 4] || authority.threads != 1 {
        return Err("gateway_root_launcher_required");
    }
    validate_descriptors(&super::launch_descriptors::observe(&BTreeSet::new())?)?;
    let bytes = read_proc("/proc/sys/fs/suid_dumpable", 16)?;
    let mode = std::str::from_utf8(&bytes)
        .map_err(|_| "gateway_kernel_invalid")?
        .trim()
        .parse()
        .map_err(|_| "gateway_kernel_invalid")?;
    validate_dump_mode(mode)?;
    // Establish the irreversible exec restriction while still root. Setting it
    // after setresuid would leave a privilege-regaining exec window during drop.
    if prctl(libc::PR_SET_NO_NEW_PRIVS, 1) != 0 || prctl(libc::PR_SET_DUMPABLE, 0) != 0 {
        return Err("gateway_privilege_drop");
    }
    Ok(())
}

fn drop_authority(principal: Principal) -> Result<(), &'static str> {
    // Bounding drops need CAP_SETPCAP, so perform them before losing root.
    // Fail closed on unknown capability width instead of retaining future bits.
    let mut ended = false;
    for capability in 0..64 {
        let present = prctl(libc::PR_CAPBSET_READ, capability);
        if present == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINVAL) {
            ended = true;
            break;
        }
        if present < 0 || prctl(libc::PR_CAPBSET_DROP, capability) != 0 {
            return Err("gateway_privilege_drop");
        }
    }
    if !ended {
        return Err("gateway_capability_width");
    }
    if prctl(libc::PR_SET_SECUREBITS, 0) != 0
        || prctl(libc::PR_SET_KEEPCAPS, 0) != 0
        || prctl(
            libc::PR_CAP_AMBIENT,
            libc::PR_CAP_AMBIENT_CLEAR_ALL as libc::c_ulong,
        ) != 0
        || unsafe { libc::setgroups(0, std::ptr::null()) } != 0
        || unsafe { libc::setresgid(principal.gid, principal.gid, principal.gid) } != 0
        || unsafe { libc::setresuid(principal.uid, principal.uid, principal.uid) } != 0
        || prctl(libc::PR_SET_DUMPABLE, 0) != 0
    {
        return Err("gateway_privilege_drop");
    }
    #[repr(C)]
    struct CapHeader {
        version: u32,
        pid: i32,
    }
    let header = CapHeader {
        version: 0x20080522,
        pid: 0,
    };
    let empty = [0u32; 6]; // Two effective/permitted/inheritable u32 ABI records.
    if unsafe { libc::syscall(libc::SYS_capset, &header, empty.as_ptr()) } != 0 {
        return Err("gateway_privilege_drop");
    }
    Ok(())
}

fn prctl(option: libc::c_int, argument: libc::c_ulong) -> libc::c_int {
    // prctl is variadic: pass its argument slots as unsigned long, including
    // zeros, rather than relying on the ABI's treatment of promoted C ints.
    unsafe {
        libc::prctl(
            option,
            argument,
            0 as libc::c_ulong,
            0 as libc::c_ulong,
            0 as libc::c_ulong,
        )
    }
}

fn publish(path: &str, core_gid: u32, intent: &GatewayIntent) -> Result<(), &'static str> {
    let path = Path::new(path);
    let parent = path
        .parent()
        .and_then(Path::to_str)
        .ok_or("gateway_intent_directory")?;
    let name = CString::new(
        path.file_name()
            .and_then(|name| name.to_str())
            .ok_or("gateway_intent_directory")?,
    )
    .map_err(|_| "gateway_intent_directory")?;
    let mut directories = SafeFiles::new();
    let info = directories.open(parent, FileKind::State)?;
    if info.uid != 0 || info.gid != core_gid || info.mode != 0o750 {
        return Err("gateway_intent_directory");
    }
    directories.confirm()?;
    let directory = directories.file(parent)?;
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o400,
        )
    };
    if fd < 0 {
        return Err("gateway_intent_create");
    }
    // Never overwrite, repair or unlink an earlier/partial intent. A failed
    // launch leaves evidence for explicit operator inspection and a fresh plan.
    let mut file = unsafe { File::from_raw_fd(fd) };
    let bytes = serde_json::to_vec(intent).map_err(|_| "gateway_intent_create")?;
    file.write_all(&bytes)
        .map_err(|_| "gateway_intent_create")?;
    if unsafe { libc::fchown(fd, 0, core_gid) } != 0 {
        return Err("gateway_intent_create");
    }
    file.sync_all().map_err(|_| "gateway_intent_create")?;
    if unsafe { libc::fchmod(fd, 0o440) } != 0 {
        return Err("gateway_intent_create");
    }
    file.sync_all().map_err(|_| "gateway_intent_create")?;
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c".".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err("gateway_intent_create");
    }
    let directory = unsafe { File::from_raw_fd(fd) };
    directory.sync_all().map_err(|_| "gateway_intent_create")
}

pub(super) fn retain_gateway_inputs(
    snapshot: &ReadOnlySnapshot,
    path: &Path,
) -> Result<(SafeFiles, String), &'static str> {
    let mut protected = ProtectedPlan::load(snapshot, path)?;
    protected
        .files
        .open(&protected.plan.intent_file, FileKind::Read)?;
    protected.files.confirm()?;
    Ok((protected.files, protected.digest))
}

pub(super) struct RuntimeContext {
    context: NamespaceContext,
    pid: u32,
    start: u64,
    boot: String,
}

impl RuntimeContext {
    pub(super) fn capture(snapshot: &ReadOnlySnapshot) -> Result<Self, &'static str> {
        snapshot.check_current_process()?;
        let pid = std::process::id();
        Ok(Self {
            context: own_context()?,
            pid,
            start: start_ticks(pid)?,
            boot: boot_id()?,
        })
    }

    pub(super) fn check(
        &self,
        snapshot: &ReadOnlySnapshot,
        plan: &Path,
    ) -> Result<(), &'static str> {
        let (_, _, _, policy) = snapshot.gateway_binding()?;
        if std::process::id() != self.pid
            || start_ticks(self.pid)? != self.start
            || boot_id()? != self.boot
        {
            return Err("runtime_process_changed");
        }
        let core = policy.core;
        let state = &snapshot.checked.manifest.state_directory;
        for _ in 0..2 {
            if own_context()? != self.context {
                return Err("runtime_namespace_changed");
            }
            check_threads(self.pid, core, &self.context)?;
            let descriptors = super::launch_descriptors::observe_runtime(
                &snapshot.files.descriptor_numbers(),
                &|number| snapshot.files.journal_descriptor(state, number, core),
            )?;
            crate::domain::launch_authority::validate_runtime_descriptors(&descriptors)?;
            snapshot.gateway_binding()?;
        }
        check_gateway(snapshot, plan)?;
        if own_context()? != self.context {
            return Err("runtime_namespace_changed");
        }
        check_threads(self.pid, core, &self.context)?;
        snapshot.gateway_binding()?;
        Ok(())
    }
}

pub(super) fn check_gateway(snapshot: &ReadOnlySnapshot, path: &Path) -> Result<(), &'static str> {
    let mut protected = ProtectedPlan::load(snapshot, path)?;
    let (generation, manifest, policy_digest, policy) = snapshot.gateway_binding()?;
    let info = protected
        .files
        .open(&protected.plan.intent_file, FileKind::Read)?;
    if info.gid != policy.core.gid || info.mode != 0o440 {
        return Err("gateway_intent_authority");
    }
    let intent: GatewayIntent = config::decode(&protected.files.read(&protected.plan.intent_file)?)
        .map_err(|_| "gateway_intent_invalid")?;
    if intent.schema != "ciel.readonly.gateway-intent.v1"
        || intent.deployment_generation != generation
        || intent.manifest_sha256 != manifest
        || intent.policy_sha256 != policy_digest
        || intent.plan_sha256 != protected.digest
        || intent.executable_sha256 != protected.plan.executable_sha256
        || intent.pid == 0
        || intent.pid > i32::MAX as u32
        || intent.start_ticks == 0
        || intent.boot_id != boot_id()?
    {
        return Err("gateway_intent_binding");
    }
    intent.context.validate()?;
    // A pinned pidfd plus boot/start ticks detects exit and PID reuse. The
    // root-owned pre-exec intent alone is deliberately insufficient.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, intent.pid as i32, 0) };
    if fd < 0 {
        return Err("gateway_process_unavailable");
    }
    let pidfd = unsafe { File::from_raw_fd(fd as i32) };
    for _ in 0..2 {
        live(&pidfd)?;
        if start_ticks(intent.pid)? != intent.start_ticks {
            return Err("gateway_process_changed");
        }
        if own_context()? != intent.context {
            return Err("gateway_namespace_mismatch");
        }
        if read_map(&format!("/proc/{}/uid_map", intent.pid))? != intent.context.uid_map
            || read_map(&format!("/proc/{}/gid_map", intent.pid))? != intent.context.gid_map
        {
            return Err("gateway_namespace_mismatch");
        }
        check_threads(intent.pid, policy.gateway, &intent.context)?;
    }
    live(&pidfd)?;
    protected.files.confirm()?;
    snapshot.gateway_binding()?;
    Ok(())
}

fn live(pidfd: &File) -> Result<(), &'static str> {
    let mut descriptor = libc::pollfd {
        fd: pidfd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    if unsafe { libc::poll(&mut descriptor, 1, 0) } != 0 {
        return Err("gateway_process_unavailable");
    }
    Ok(())
}

fn check_threads(
    pid: u32,
    principal: Principal,
    context: &NamespaceContext,
) -> Result<(), &'static str> {
    let before = task_ids(pid)?;
    for tid in &before {
        let authority = service_launcher::status::parse(&read_proc(
            &format!("/proc/{pid}/task/{tid}/status"),
            65536,
        )?)?;
        authority.validate_identity(principal)?;
        if read_map(&format!("/proc/{pid}/task/{tid}/uid_map"))? != context.uid_map
            || read_map(&format!("/proc/{pid}/task/{tid}/gid_map"))? != context.gid_map
        {
            return Err("gateway_namespace_mismatch");
        }
        if authority.threads as usize != before.len() {
            return Err("gateway_process_changed");
        }
    }
    if task_ids(pid)? != before {
        return Err("gateway_process_changed");
    }
    Ok(())
}

fn task_ids(pid: u32) -> Result<BTreeSet<u32>, &'static str> {
    let mut ids = BTreeSet::new();
    for entry in
        std::fs::read_dir(format!("/proc/{pid}/task")).map_err(|_| "gateway_process_unavailable")?
    {
        let entry = entry.map_err(|_| "gateway_process_unavailable")?;
        let tid = entry
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|tid| *tid != 0)
            .ok_or("gateway_process_unavailable")?;
        ids.insert(tid);
        if ids.len() > 4096 {
            return Err("gateway_thread_limit");
        }
    }
    if ids.is_empty() {
        return Err("gateway_process_unavailable");
    }
    Ok(ids)
}

pub(super) fn read_proc(path: &str, limit: u64) -> Result<Vec<u8>, &'static str> {
    let file = File::open(path).map_err(|_| "gateway_kernel_unavailable")?;
    filesystem(&file, libc::PROC_SUPER_MAGIC as libc::c_long)?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "gateway_kernel_unavailable")?;
    if bytes.len() as u64 > limit {
        return Err("gateway_kernel_invalid");
    }
    Ok(bytes)
}

fn filesystem(file: &File, expected: libc::c_long) -> Result<(), &'static str> {
    let mut value = std::mem::MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::fstatfs(file.as_raw_fd(), value.as_mut_ptr()) } != 0
        || unsafe { value.assume_init() }.f_type != expected
    {
        return Err("gateway_kernel_unavailable");
    }
    Ok(())
}

fn boot_id() -> Result<String, &'static str> {
    let bytes = read_proc("/proc/sys/kernel/random/boot_id", 64)?;
    let value = std::str::from_utf8(&bytes)
        .map_err(|_| "gateway_kernel_invalid")?
        .trim();
    uuid::Uuid::parse_str(value).map_err(|_| "gateway_kernel_invalid")?;
    Ok(value.into())
}

fn start_ticks(pid: u32) -> Result<u64, &'static str> {
    let bytes = read_proc(&format!("/proc/{pid}/stat"), 8192)?;
    parse_start_ticks(pid, &bytes)
}

fn parse_start_ticks(pid: u32, bytes: &[u8]) -> Result<u64, &'static str> {
    let text = std::str::from_utf8(bytes).map_err(|_| "gateway_kernel_invalid")?;
    let (prefix, rest) = text.rsplit_once(") ").ok_or("gateway_kernel_invalid")?;
    if !prefix.starts_with(&format!("{pid} (")) {
        return Err("gateway_kernel_invalid");
    }
    let fields: Vec<_> = rest.split_ascii_whitespace().collect();
    if matches!(fields.first().copied(), None | Some("Z" | "X" | "x")) {
        return Err("gateway_process_unavailable");
    }
    fields
        .get(19)
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .ok_or("gateway_kernel_invalid")
}

fn own_context() -> Result<NamespaceContext, &'static str> {
    // A procfs mounted from a different PID namespace must not reinterpret the
    // launch PID as an unrelated process in the mount's numeric PID space.
    parse_start_ticks(std::process::id(), &read_proc("/proc/self/stat", 8192)?)?;
    let mut namespaces = BTreeMap::new();
    for name in NAMESPACES {
        let file = File::open(format!("/proc/thread-self/ns/{name}"))
            .map_err(|_| "gateway_kernel_unavailable")?;
        filesystem(&file, 0x6e736673)?; // Linux NSFS_MAGIC.
        let info = file.metadata().map_err(|_| "gateway_kernel_unavailable")?;
        namespaces.insert(
            name.into(),
            NamespaceId {
                device: info.dev(),
                inode: info.ino(),
            },
        );
    }
    let context = NamespaceContext {
        namespaces,
        uid_map: read_map("/proc/thread-self/uid_map")?,
        gid_map: read_map("/proc/thread-self/gid_map")?,
    };
    context.validate()?;
    Ok(context)
}

fn read_map(path: &str) -> Result<Vec<[u32; 3]>, &'static str> {
    let bytes = read_proc(path, 16384)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "gateway_kernel_invalid")?;
    text.lines()
        .map(|line| {
            let row: Result<Vec<_>, _> = line
                .split_ascii_whitespace()
                .map(|value| {
                    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
                        return Err("gateway_kernel_invalid");
                    }
                    value.parse::<u32>().map_err(|_| "gateway_kernel_invalid")
                })
                .collect();
            row?.try_into().map_err(|_| "gateway_kernel_invalid")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_start_binding_handles_comm_parentheses_and_rejects_wrong_pid_or_dead_state() {
        let rest = format!("S {} 42 0 0\n", vec!["0"; 18].join(" "));
        assert_eq!(
            parse_start_ticks(
                123,
                format!("123 (name) with ) parentheses) {rest}").as_bytes()
            ),
            Ok(42)
        );
        for bytes in [
            format!("124 (other) {rest}"),
            format!("123 (dead) {}", rest.replacen('S', "Z", 1)),
            "123 (short) S 0".into(),
        ] {
            assert!(parse_start_ticks(123, bytes.as_bytes()).is_err());
        }
    }

    #[test]
    fn actual_self_procfs_pid_namespace_maps_and_start_binding_are_consistent() {
        own_context().unwrap().validate().unwrap();
        let pid = std::process::id();
        assert_eq!(
            start_ticks(pid).unwrap(),
            parse_start_ticks(pid, &read_proc("/proc/self/stat", 8192).unwrap()).unwrap()
        );
    }
}
