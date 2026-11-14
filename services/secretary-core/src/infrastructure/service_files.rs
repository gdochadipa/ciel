//! Protected inode inspection; guarded runtime preparation explicitly promotes secrets.
use super::service_check::{FileKind, Files, Stamp};
use std::{collections::BTreeMap, fs::File, io::Read};

pub(super) struct SafeFiles {
    held: BTreeMap<String, Held>,
    runtime: bool,
}

struct Held {
    file: File,
    ancestors: Vec<File>,
    stamps: Vec<Stamp>,
    kind: FileKind,
}

impl SafeFiles {
    pub(super) fn enter_runtime(&mut self) -> Result<(), &'static str> {
        self.confirm()?;
        self.runtime = true;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub(super) fn journal_descriptor(
        &self,
        state: &str,
        descriptor: i32,
        core: crate::domain::service_config::Principal,
    ) -> Result<bool, &'static str> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let held = self.held.get(state).ok_or("runtime_state_unavailable")?;
        if held.kind != FileKind::State {
            return Err("runtime_state_unavailable");
        }
        let mut observed = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(descriptor, observed.as_mut_ptr()) } != 0 {
            return Err("runtime_descriptors_unavailable");
        }
        let observed = unsafe { observed.assume_init() };
        for name in [
            c"secretary.lock",
            c"secretary.sqlite3",
            c"secretary.sqlite3-wal",
            c"secretary.sqlite3-shm",
        ] {
            // Resolve only fixed journal names under the held private directory.
            // An O_PATH open cannot invoke a device open handler or read a body.
            let fd = unsafe {
                libc::openat(
                    held.file.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
                    continue;
                }
                return Err("runtime_state_unavailable");
            }
            let file = unsafe { File::from_raw_fd(fd) };
            let metadata = file.metadata().map_err(|_| "runtime_state_unavailable")?;
            let info = stamp(&metadata);
            if info.device == observed.st_dev && info.inode == observed.st_ino {
                if !metadata.is_file()
                    || info.uid != core.uid
                    || info.gid != core.gid
                    || info.mode != 0o600
                    || info.links != 1
                {
                    return Err("runtime_journal_authority");
                }
                no_acl(&file)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    pub(super) fn check_tree<'a>(
        &mut self,
        root: &str,
        files: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), &'static str> {
        use std::{collections::BTreeSet, os::fd::AsRawFd};
        let mut tree: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        for relative in files {
            let mut directory = root.to_owned();
            let mut components = relative.split('/').peekable();
            while let Some(component) = components.next() {
                tree.entry(directory.clone()).or_default().insert(component);
                if components.peek().is_some() {
                    directory.push('/');
                    directory.push_str(component);
                }
            }
        }
        for (directory, expected) in tree {
            administrator_directory(self.open(&directory, FileKind::State)?)?;
            let descriptor = self.file(&directory)?.as_raw_fd();
            let mut actual = BTreeSet::new();
            let entries = std::fs::read_dir(format!("/proc/self/fd/{descriptor}"))
                .map_err(|_| "release_tree_unavailable")?;
            for entry in entries {
                let name = entry.map_err(|_| "release_tree_unavailable")?.file_name();
                if !name.to_str().is_some_and(|name| expected.contains(name)) {
                    return Err("release_tree_mismatch");
                }
                actual.insert(name);
            }
            if actual.len() != expected.len() {
                return Err("release_tree_mismatch");
            }
        }
        self.confirm()
    }

    #[cfg(target_os = "linux")]
    pub(super) fn input_stamp(&self, path: &str) -> Result<Stamp, &'static str> {
        self.held
            .get(path)
            .and_then(|held| held.stamps.last().copied())
            .ok_or("file_unavailable")
    }

    #[cfg(target_os = "linux")]
    pub(super) fn retain(&mut self, other: Self) -> Result<(), &'static str> {
        if other.held.keys().any(|path| self.held.contains_key(path)) {
            return Err("input_alias");
        }
        self.held.extend(other.held);
        self.confirm()
    }

    pub(super) fn read_private(
        &mut self,
        path: &str,
        limit: usize,
    ) -> Result<Vec<u8>, &'static str> {
        self.confirm()?;
        let held = self.held.get_mut(path).ok_or("file_unavailable")?;
        if held.kind != FileKind::Metadata {
            return Err("private_input_already_read");
        }
        let expected = *held.stamps.last().ok_or("file_unavailable")?;
        if expected.size > limit as u64 {
            return Err("runtime_secret_too_large");
        }
        // Promote the already-inspected inode, never a newly resolved filename.
        // Retain this CLOEXEC handle so subsequent authority checks include it.
        held.file = readable(
            held.file.try_clone().map_err(|_| "file_unavailable")?,
            expected,
        )?;
        held.kind = FileKind::Read;
        let mut bytes = Vec::new();
        (&mut held.file)
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "file_unavailable")?;
        if bytes.len() > limit {
            return Err("runtime_secret_too_large");
        }
        self.confirm()?;
        Ok(bytes)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn file(&self, path: &str) -> Result<&File, &'static str> {
        self.held
            .get(path)
            .map(|held| &held.file)
            .ok_or("file_unavailable")
    }

    pub(super) fn new() -> Self {
        Self {
            held: BTreeMap::new(),
            runtime: false,
        }
    }

    #[cfg(target_os = "linux")]
    pub(super) fn descriptor_numbers(&self) -> std::collections::BTreeSet<i32> {
        use std::os::fd::AsRawFd;
        self.held
            .values()
            .flat_map(|held| {
                held.ancestors
                    .iter()
                    .chain([&held.file])
                    .map(AsRawFd::as_raw_fd)
            })
            .collect()
    }
}

impl Files for SafeFiles {
    fn open(&mut self, path: &str, kind: FileKind) -> Result<Stamp, &'static str> {
        if self.held.contains_key(path) {
            return Err("input_alias");
        }
        let held = open(path, kind)?;
        let stamp = *held.stamps.last().ok_or("file_unavailable")?;
        self.held.insert(path.into(), held);
        Ok(stamp)
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>, &'static str> {
        let held = self.held.get_mut(path).ok_or("file_unavailable")?;
        if held.kind != FileKind::Read {
            return Err("metadata_only_input");
        }
        let mut bytes = Vec::new();
        (&mut held.file)
            .take(1_048_577)
            .read_to_end(&mut bytes)
            .map_err(|_| "file_unavailable")?;
        if bytes.len() > 1_048_576 {
            return Err("input_too_large");
        }
        Ok(bytes)
    }

    fn hash(&mut self, path: &str) -> Result<String, &'static str> {
        use sha2::{Digest, Sha256};
        let held = self.held.get_mut(path).ok_or("file_unavailable")?;
        if held.kind != FileKind::Read {
            return Err("metadata_only_input");
        }
        let mut hash = Sha256::new();
        let mut bytes = [0u8; 16384];
        let mut total = 0usize;
        loop {
            let count = held
                .file
                .read(&mut bytes)
                .map_err(|_| "artifact_unavailable")?;
            if count == 0 {
                break;
            }
            total = total.checked_add(count).ok_or("artifact_too_large")?;
            if total > 128 * 1024 * 1024 {
                return Err("artifact_too_large");
            }
            hash.update(&bytes[..count]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }

    fn confirm(&self) -> Result<(), &'static str> {
        for (path, held) in &self.held {
            // Compare both held descriptors and the current rooted name walk.
            // An administrator's concurrent replacement must not mix snapshots.
            let current: Result<Vec<_>, _> = held
                .ancestors
                .iter()
                .chain([&held.file])
                .map(|file| file.metadata().map(|metadata| stamp(&metadata)))
                .collect();
            // Revalidation needs only authority/identity metadata. Reopening a
            // promoted secret for reading would needlessly reacquire body access
            // on every guard, despite consuming its contents only once.
            let kind = if held.kind == FileKind::State {
                FileKind::State
            } else {
                FileKind::Metadata
            };
            if !same_stamps(
                &current.map_err(|_| "file_changed")?,
                &held.stamps,
                self.runtime,
            ) || !same_stamps(&open(path, kind)?.stamps, &held.stamps, self.runtime)
            {
                return Err("file_changed");
            }
        }
        Ok(())
    }

    fn directories(&self) -> Vec<Stamp> {
        self.held
            .values()
            .flat_map(|held| held.stamps.iter().copied())
            .filter(|info| info.directory)
            .collect()
    }
}

fn same_stamps(observed: &[Stamp], expected: &[Stamp], runtime: bool) -> bool {
    observed.len() == expected.len()
        && observed.iter().zip(expected).all(|(now, before)| {
            if runtime && before.directory {
                now.directory
                    && now.uid == before.uid
                    && now.gid == before.gid
                    && now.mode == before.mode
                    && now.device == before.device
                    && now.inode == before.inode
            } else {
                now == before
            }
        })
}

#[cfg(unix)]
fn stamp(metadata: &std::fs::Metadata) -> Stamp {
    use std::os::unix::fs::MetadataExt;
    Stamp {
        uid: metadata.uid(),
        gid: metadata.gid(),
        mode: metadata.mode() & 0o7777,
        device: metadata.dev(),
        inode: metadata.ino(),
        links: metadata.nlink(),
        directory: metadata.is_dir(),
        size: metadata.len(),
        modified: (metadata.mtime(), metadata.mtime_nsec()),
        changed: (metadata.ctime(), metadata.ctime_nsec()),
    }
}

#[cfg(not(unix))]
fn stamp(_: &std::fs::Metadata) -> Stamp {
    unreachable!("unsupported platform")
}

#[cfg(target_os = "linux")]
fn metadata_flags() -> Result<i32, &'static str> {
    Ok(libc::O_PATH)
}

#[cfg(not(target_os = "linux"))]
fn metadata_flags() -> Result<i32, &'static str> {
    Err("file_authority_unsupported")
}

#[cfg(unix)]
fn open(path: &str, kind: FileKind) -> Result<Held, &'static str> {
    use std::{
        ffi::CString,
        fs::OpenOptions,
        os::fd::{AsRawFd, FromRawFd},
        os::unix::fs::OpenOptionsExt,
    };
    // v1's ACL/metadata-only policy is deliberately Linux-specific.
    let metadata_flags = metadata_flags()?;
    if !crate::domain::service_config::absolute_path(path) {
        return Err("manifest_path");
    }
    let root = OpenOptions::new()
        .read(true)
        .custom_flags(metadata_flags | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/")
        .map_err(|_| "file_unavailable")?;
    let root_stamp = stamp(&root.metadata().map_err(|_| "file_unavailable")?);
    administrator_directory(root_stamp)?;
    no_acl(&root)?;
    let mut ancestors = vec![root];
    let mut stamps = vec![root_stamp];
    let components: Vec<_> = path[1..].split('/').collect();
    for (index, component) in components.iter().enumerate() {
        let last = index + 1 == components.len();
        // Inspect every inode before any readable open. O_NONBLOCK alone would
        // not prevent a device's open handler from running before type checks.
        let flags = metadata_flags
            | if !last || kind == FileKind::State {
                libc::O_DIRECTORY
            } else {
                0
            };
        let name = CString::new(*component).map_err(|_| "manifest_path")?;
        let parent = ancestors.last().ok_or("file_unavailable")?;
        // The parent descriptor stays alive; NOFOLLOW applies at every hop.
        let descriptor = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            )
        };
        if descriptor < 0 {
            return Err("file_unavailable");
        }
        // openat returned a new owned descriptor; File closes it on all exits.
        let file = unsafe { File::from_raw_fd(descriptor) };
        let metadata = file.metadata().map_err(|_| "file_unavailable")?;
        let info = stamp(&metadata);
        if last {
            if kind == FileKind::State {
                if !metadata.is_dir() {
                    return Err("unsafe_state");
                }
            } else if !metadata.is_file()
                || info.uid != 0
                || info.links != 1
                || info.mode & 0o7022 != 0
            {
                return Err("unsafe_input");
            }
            no_acl(&file)?;
            let file = if kind == FileKind::Read {
                readable(file, info)?
            } else {
                file
            };
            stamps.push(info);
            return Ok(Held {
                file,
                ancestors,
                stamps,
                kind,
            });
        }
        administrator_directory(info)?;
        no_acl(&file)?;
        stamps.push(info);
        ancestors.push(file);
    }
    Err("manifest_path")
}

#[cfg(unix)]
fn readable(file: File, expected: Stamp) -> Result<File, &'static str> {
    use std::{fs::OpenOptions, os::fd::AsRawFd, os::unix::fs::OpenOptionsExt};
    let metadata = file.metadata().map_err(|_| "file_unavailable")?;
    if !metadata.is_file() {
        return Err("unsafe_input");
    }
    if stamp(&metadata) != expected {
        return Err("file_changed");
    }
    // This internal procfs reference resolves the held regular inode, not a
    // caller-controlled name. NOFOLLOW would reject the required procfs link.
    let readable = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(format!("/proc/self/fd/{}", file.as_raw_fd()))
        .map_err(|_| "file_unavailable")?;
    if stamp(&readable.metadata().map_err(|_| "file_unavailable")?) != expected {
        return Err("file_changed");
    }
    Ok(readable)
}

#[cfg(not(unix))]
fn open(_: &str, _: FileKind) -> Result<Held, &'static str> {
    Err("file_authority_unsupported")
}

#[cfg(not(unix))]
fn readable(_: File, _: Stamp) -> Result<File, &'static str> {
    Err("file_authority_unsupported")
}

fn administrator_directory(info: Stamp) -> Result<(), &'static str> {
    if !info.directory || info.uid != 0 || info.mode & 0o7022 != 0 {
        return Err("unsafe_ancestor");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn no_acl(file: &File) -> Result<(), &'static str> {
    use std::{ffi::CString, os::fd::AsRawFd};
    // getxattr through this held procfs descriptor inspects the exact O_PATH
    // inode without opening a token/key body. Missing procfs fails closed.
    let path = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd()))
        .map_err(|_| "acl_inspection_unsupported")?;
    for name in [
        b"system.posix_acl_access\0".as_slice(),
        b"system.posix_acl_default\0".as_slice(),
    ] {
        // Both C strings remain alive; a null buffer asks for metadata size only.
        let result =
            unsafe { libc::getxattr(path.as_ptr(), name.as_ptr().cast(), std::ptr::null_mut(), 0) };
        if result >= 0 {
            return Err("extended_acl_forbidden");
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::ENODATA) {
            return Err("acl_inspection_unsupported");
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn no_acl(_: &File) -> Result<(), &'static str> {
    Err("acl_inspection_unsupported")
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::fs::OpenOptionsExt;

    #[test]
    fn metadata_descriptor_can_inspect_acl_without_reading_a_body() {
        let path =
            std::env::temp_dir().join(format!("secretary-metadata-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"synthetic unread body").unwrap();
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)
            .unwrap();
        let acl = no_acl(&file);
        let read = file.read(&mut [0u8; 1]);
        std::fs::remove_file(path).unwrap();
        assert_eq!(acl, Ok(()));
        assert_eq!(read.unwrap_err().raw_os_error(), Some(libc::EBADF));
    }

    #[test]
    fn special_device_is_rejected_before_obtaining_a_readable_descriptor() {
        let metadata = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/dev/null")
            .unwrap();
        let info = stamp(&metadata.metadata().unwrap());
        assert_eq!(readable(metadata, info).unwrap_err(), "unsafe_input");
    }
}

#[cfg(test)]
mod stamp_tests {
    use super::*;
    #[test]
    fn runtime_directory_contents_can_change_but_identity_authority_and_inputs_cannot() {
        let directory = Stamp {
            uid: 1,
            gid: 1,
            mode: 0o700,
            device: 2,
            inode: 3,
            links: 2,
            directory: true,
            size: 10,
            modified: (1, 0),
            changed: (1, 0),
        };
        let changed = Stamp {
            size: 100,
            links: 4,
            modified: (2, 0),
            changed: (2, 0),
            ..directory
        };
        assert!(!same_stamps(&[changed], &[directory], false));
        assert!(same_stamps(&[changed], &[directory], true));
        for unsafe_change in [
            Stamp { uid: 2, ..changed },
            Stamp { gid: 2, ..changed },
            Stamp {
                mode: 0o750,
                ..changed
            },
            Stamp {
                inode: 4,
                ..changed
            },
            Stamp {
                device: 4,
                ..changed
            },
            Stamp {
                directory: false,
                ..changed
            },
        ] {
            assert!(!same_stamps(&[unsafe_change], &[directory], true));
        }
        let input = Stamp {
            directory: false,
            links: 1,
            mode: 0o440,
            ..directory
        };
        assert!(!same_stamps(
            &[Stamp {
                modified: (2, 0),
                ..input
            }],
            &[input],
            true
        ));
    }
}
