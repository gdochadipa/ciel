//! Inspect existing descriptors without reading bodies or invoking device opens.
use crate::domain::launch_authority::{Descriptor, DescriptorAccess, DescriptorKind};
use std::{collections::BTreeSet, fs::File, os::fd::AsRawFd};

// Linux UAPI include/uapi/linux/magic.h; named filesystem FIFOs are not anonymous
// pipes and must not be mistaken for safe standard I/O to a launcher-controlled log.
const PIPEFS_MAGIC: libc::c_long = 0x5049_5045;
const MAX_DESCRIPTORS: usize = 65_536;

pub(super) fn observe(known_inputs: &BTreeSet<i32>) -> Result<Vec<Descriptor>, &'static str> {
    observe_with(known_inputs, None)
}

type RuntimeClassifier<'a> = dyn Fn(i32) -> Result<DescriptorKind, &'static str> + 'a;

pub(super) fn observe_runtime(
    known_inputs: &BTreeSet<i32>,
    journal: &dyn Fn(i32) -> Result<bool, &'static str>,
) -> Result<Vec<Descriptor>, &'static str> {
    observe_with(known_inputs, Some(&|number| runtime_kind(number, journal)))
}

fn observe_with(
    known_inputs: &BTreeSet<i32>,
    runtime: Option<&RuntimeClassifier<'_>>,
) -> Result<Vec<Descriptor>, &'static str> {
    let directory =
        File::open("/proc/thread-self/fd").map_err(|_| "launcher_descriptors_unavailable")?;
    if filesystem(directory.as_raw_fd())? != libc::PROC_SUPER_MAGIC as libc::c_long {
        return Err("launcher_descriptors_unavailable");
    }
    let identity = metadata(directory.as_raw_fd())?;
    let entries = std::fs::read_dir("/proc/thread-self/fd")
        .map_err(|_| "launcher_descriptors_unavailable")?;
    let mut descriptors = Vec::new();
    for entry in entries {
        if descriptors.len() >= MAX_DESCRIPTORS {
            return Err("launcher_descriptor_limit");
        }
        let name = entry
            .map_err(|_| "launcher_descriptors_unavailable")?
            .file_name();
        let name = name.to_str().ok_or("launcher_descriptors_unavailable")?;
        if !name.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("launcher_descriptors_unavailable");
        }
        let number: i32 = name
            .parse()
            .map_err(|_| "launcher_descriptors_unavailable")?;
        let info = metadata(number)?;
        // Inspect existing handles. Reopening an inherited device or file could
        // trigger device behavior or open a secret body before rejecting it.
        let flags = unsafe { libc::fcntl(number, libc::F_GETFL) };
        let descriptor_flags = unsafe { libc::fcntl(number, libc::F_GETFD) };
        if flags < 0 || descriptor_flags < 0 {
            return Err("launcher_descriptors_unavailable");
        }
        let access = if flags & libc::O_PATH != 0 {
            DescriptorAccess::Metadata
        } else {
            match flags & libc::O_ACCMODE {
                libc::O_RDONLY => DescriptorAccess::Read,
                libc::O_WRONLY => DescriptorAccess::Write,
                libc::O_RDWR => DescriptorAccess::ReadWrite,
                _ => return Err("launcher_descriptors_unavailable"),
            }
        };
        let kind = if known_inputs.contains(&number) {
            DescriptorKind::ProtectedInput
        } else if info.st_mode & libc::S_IFMT == libc::S_IFDIR
            && info.st_dev == identity.st_dev
            && info.st_ino == identity.st_ino
            && filesystem(number)? == libc::PROC_SUPER_MAGIC as libc::c_long
        {
            // read_dir has no public fd accessor. Match only handles to this
            // exact held procfs directory, never a caller-controlled path string.
            DescriptorKind::Introspection
        } else if info.st_mode & libc::S_IFMT == libc::S_IFCHR
            && info.st_rdev == libc::makedev(1, 3)
        {
            DescriptorKind::Null
        } else if info.st_mode & libc::S_IFMT == libc::S_IFIFO
            && filesystem(number)? == PIPEFS_MAGIC
        {
            DescriptorKind::Pipe
        } else {
            match runtime {
                Some(classify) => classify(number)?,
                None => DescriptorKind::Other,
            }
        };
        descriptors.push(Descriptor {
            number,
            kind,
            access,
            close_on_exec: descriptor_flags & libc::FD_CLOEXEC != 0,
        });
    }
    descriptors.sort_unstable_by_key(|descriptor| descriptor.number);
    Ok(descriptors)
}

fn runtime_kind(
    number: i32,
    journal: &dyn Fn(i32) -> Result<bool, &'static str>,
) -> Result<DescriptorKind, &'static str> {
    let info = metadata(number)?;
    if info.st_mode & libc::S_IFMT == libc::S_IFREG && journal(number)? {
        return Ok(DescriptorKind::RuntimeJournal);
    }
    if info.st_mode & libc::S_IFMT == libc::S_IFSOCK {
        let option = |name| {
            let mut value = 0i32;
            let mut size = std::mem::size_of_val(&value) as libc::socklen_t;
            // getsockopt observes the existing handle; it adopts no connection.
            if unsafe {
                libc::getsockopt(
                    number,
                    libc::SOL_SOCKET,
                    name,
                    (&mut value as *mut i32).cast(),
                    &mut size,
                )
            } != 0
                || size as usize != std::mem::size_of_val(&value)
            {
                return Err("runtime_descriptors_unavailable");
            }
            Ok(value)
        };
        if matches!(option(libc::SO_DOMAIN)?, libc::AF_INET | libc::AF_INET6)
            && matches!(
                (option(libc::SO_TYPE)?, option(libc::SO_PROTOCOL)?),
                (libc::SOCK_STREAM, libc::IPPROTO_TCP) | (libc::SOCK_DGRAM, libc::IPPROTO_UDP)
            )
        {
            return Ok(DescriptorKind::RuntimeSocket);
        }
        if option(libc::SO_DOMAIN)? == libc::AF_UNIX
            && option(libc::SO_TYPE)? == libc::SOCK_STREAM
            && same_process_pair(number)?
        {
            return Ok(DescriptorKind::RuntimeSocket);
        }
    }
    if filesystem(number)? == 0x0904_1934 {
        let target = std::fs::read_link(format!("/proc/thread-self/fd/{number}"))
            .map_err(|_| "runtime_descriptors_unavailable")?;
        if matches!(
            target.to_str(),
            Some("anon_inode:[eventpoll]" | "anon_inode:[eventfd]")
        ) {
            return Ok(DescriptorKind::RuntimePoll);
        }
    }
    Ok(DescriptorKind::Other)
}

fn same_process_pair(number: i32) -> Result<bool, &'static str> {
    let mut credentials = std::mem::MaybeUninit::<libc::ucred>::uninit();
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            number,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            credentials.as_mut_ptr().cast(),
            &mut size,
        )
    } != 0
        || size as usize != std::mem::size_of::<libc::ucred>()
    {
        return Err("runtime_descriptors_unavailable");
    }
    let credentials = unsafe { credentials.assume_init() };
    if credentials.pid != unsafe { libc::getpid() }
        || credentials.uid != unsafe { libc::getuid() }
        || credentials.gid != unsafe { libc::getgid() }
    {
        return Ok(false);
    }
    for peer in [false, true] {
        let mut address = std::mem::MaybeUninit::<libc::sockaddr_un>::zeroed();
        let mut length = std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t;
        let status = unsafe {
            if peer {
                libc::getpeername(number, address.as_mut_ptr().cast(), &mut length)
            } else {
                libc::getsockname(number, address.as_mut_ptr().cast(), &mut length)
            }
        };
        if status != 0 {
            return Err("runtime_descriptors_unavailable");
        }
        // Both ends must be unnamed. A same-UID daemon or abstract/filesystem
        // listener is not an internal signal-driver socketpair.
        if length as usize != std::mem::size_of::<libc::sa_family_t>()
            || unsafe { address.assume_init() }.sun_family as i32 != libc::AF_UNIX
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn metadata(number: i32) -> Result<libc::stat, &'static str> {
    let mut value = std::mem::MaybeUninit::<libc::stat>::uninit();
    // fstat initializes the output on success; no ownership changes or body reads.
    if unsafe { libc::fstat(number, value.as_mut_ptr()) } != 0 {
        return Err("launcher_descriptors_unavailable");
    }
    Ok(unsafe { value.assume_init() })
}

fn filesystem(number: i32) -> Result<libc::c_long, &'static str> {
    let mut value = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // fstatfs initializes the output only on success; the caller keeps its fd alive.
    if unsafe { libc::fstatfs(number, value.as_mut_ptr()) } != 0 {
        return Err("launcher_descriptors_unavailable");
    }
    Ok(unsafe { value.assume_init() }.f_type)
}
