//! Compatibility checks for the protected manifest startup path.
use super::config;
use flate2::bufread::GzDecoder;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read};

const PACKAGE_FILES: [&str; 3] = ["package.json", "openclaw.plugin.json", "dist/index.js"];
const ARCHIVE_LIMIT: u64 = 16 * 1024 * 1024;

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn octal(field: &[u8]) -> Result<usize, &'static str> {
    let value = std::str::from_utf8(field).map_err(|_| "release_archive_invalid")?;
    let value = value.trim_matches([' ', '\0']);
    usize::from_str_radix(value, 8).map_err(|_| "release_archive_invalid")
}

// The pinned SDK emits only short USTAR names and regular files/directories.
// Reject extensions, links and ambiguous headers. Nothing is extracted to disk.
fn package_members(bytes: &[u8]) -> Result<BTreeMap<&str, &[u8]>, &'static str> {
    let mut rest = bytes;
    let mut members = BTreeMap::new();
    let mut directories = std::collections::BTreeSet::new();
    loop {
        let (header, next) = rest
            .split_at_checked(512)
            .ok_or("release_archive_invalid")?;
        if header.iter().all(|byte| *byte == 0) {
            if next.len() < 512 || next.iter().any(|byte| *byte != 0) {
                return Err("release_archive_invalid");
            }
            break;
        }
        let checksum: usize = header[..148]
            .iter()
            .chain(&header[156..])
            .map(|byte| usize::from(*byte))
            .sum::<usize>()
            + 8 * 32;
        if &header[257..265] != b"ustar\x0000"
            || header[157..257]
                .iter()
                .chain(&header[345..])
                .any(|byte| *byte != 0)
            || octal(&header[148..156])? != checksum
        {
            return Err("release_archive_invalid");
        }
        let size = octal(&header[124..136])?;
        let padded = size
            .checked_next_multiple_of(512)
            .ok_or("release_archive_invalid")?;
        let (payload, remaining) = next
            .split_at_checked(padded)
            .ok_or("release_archive_invalid")?;
        rest = remaining;
        let (name, padding) = header[..100].split_at(
            header[..100]
                .iter()
                .position(|byte| *byte == 0)
                .ok_or("release_archive_invalid")?,
        );
        if padding.iter().any(|byte| *byte != 0) || payload[size..].iter().any(|byte| *byte != 0) {
            return Err("release_archive_invalid");
        }
        let name = std::str::from_utf8(name).map_err(|_| "release_archive_invalid")?;
        match header[156] {
            b'5' if matches!(name, "package/" | "package/dist/") && size == 0 => {
                if !directories.insert(name) {
                    return Err("release_archive_invalid");
                }
            }
            b'0' if size > 0 => {
                let name = name
                    .strip_prefix("package/")
                    .ok_or("release_archive_invalid")?;
                if !PACKAGE_FILES.contains(&name)
                    || members.insert(name, &payload[..size]).is_some()
                {
                    return Err("release_archive_invalid");
                }
            }
            _ => return Err("release_archive_invalid"),
        }
    }
    if members.len() != PACKAGE_FILES.len() {
        return Err("release_archive_invalid");
    }
    Ok(members)
}

fn package_hashes(compressed: &[u8]) -> Result<BTreeMap<&'static str, String>, &'static str> {
    let mut decoder = GzDecoder::new(compressed);
    let mut bytes = Vec::new();
    (&mut decoder)
        .take(ARCHIVE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "release_archive_invalid")?;
    if bytes.len() as u64 > ARCHIVE_LIMIT || !decoder.into_inner().is_empty() {
        return Err("release_archive_invalid");
    }
    let members = package_members(&bytes)?;
    let metadata: Value =
        config::decode(members["package.json"]).map_err(|_| "release_plugin_contract")?;
    let manifest: Value =
        config::decode(members["openclaw.plugin.json"]).map_err(|_| "release_plugin_contract")?;
    let expected_manifest: Value = config::decode(include_bytes!(
        "../../../../plugins/secretary-tools/openclaw.plugin.json"
    ))
    .map_err(|_| "release_plugin_contract")?;
    let expected_metadata = json!({
        "name": "@personal-secretary/openclaw-tools",
        "version": expected_manifest["version"],
        "type": "module",
        "license": "MIT",
        "peerDependencies": {"openclaw": "2026.10.7"},
        "openclaw": {"extensions": ["./dist/index.js"]}
    });
    if manifest != expected_manifest || metadata != expected_metadata {
        return Err("release_plugin_contract");
    }
    Ok(PACKAGE_FILES
        .into_iter()
        .map(|name| (name, sha256(members[name])))
        .collect())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub(super) fn check(
    snapshot: &mut super::service_check::ReadOnlySnapshot,
    plugin_directory: &std::path::Path,
) -> Result<(), &'static str> {
    use super::service_check::{FileKind, Files, ARTIFACTS};
    use crate::domain::service_config::absolute_path;
    use std::{ffi::CStr, os::unix::fs::MetadataExt, path::Path};

    let release = &snapshot.checked.release;
    let machine = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        _ => return Err("release_target_incompatible"),
    };
    // glibc returns a static NUL-terminated version string.
    let glibc = unsafe { CStr::from_ptr(libc::gnu_get_libc_version()) }
        .to_str()
        .map_err(|_| "release_target_incompatible")?;
    if release.build_host.machine != machine
        || release.build_host.libc != ["glibc", glibc]
        || !build_tools(&release.tool_versions, machine)
    {
        return Err("release_target_incompatible");
    }
    for (name, bytes) in [
        (
            "Cargo.lock",
            include_bytes!("../../../../Cargo.lock").as_slice(),
        ),
        (
            "bun.lock",
            include_bytes!("../../../../bun.lock").as_slice(),
        ),
    ] {
        if release.lock_hashes[name] != sha256(bytes) {
            return Err("release_lock_mismatch");
        }
    }
    // Stage copies these files byte-for-byte. Bind the shipped templates to
    // the running Core's reviewed source, including the inactive defaults.
    for (name, bytes) in [
        (
            "templates/service.env.example",
            include_bytes!("../../../../.env.example").as_slice(),
        ),
        (
            "templates/qualification.example.json",
            include_bytes!("../../../../deploy/qualification.example.json").as_slice(),
        ),
        (
            "templates/registry.example.json",
            include_bytes!("../../../../config/workspace.example.json").as_slice(),
        ),
        (
            "templates/openclaw.example.json",
            include_bytes!("../../../../config/openclaw.example.json").as_slice(),
        ),
        (
            "templates/secretary-core.service",
            include_bytes!("../../../../deploy/secretary-core.service").as_slice(),
        ),
    ] {
        if release.artifact_hashes[name] != sha256(bytes) {
            return Err("release_template_mismatch");
        }
    }
    let root = Path::new(&snapshot.checked.manifest.release_file)
        .parent()
        .and_then(Path::to_str)
        .filter(|p| absolute_path(p))
        .ok_or("release_invalid")?;
    if snapshot.checked.manifest.release_file != format!("{root}/release.json") {
        return Err("release_invalid");
    }
    let core = snapshot
        .files
        .file(&format!("{root}/bin/secretary-core"))?
        .metadata()
        .map_err(|_| "release_executable_unavailable")?;
    let running =
        std::fs::metadata("/proc/self/exe").map_err(|_| "release_executable_unavailable")?;
    if (core.dev(), core.ino()) != (running.dev(), running.ino()) {
        return Err("release_executable_mismatch");
    }

    snapshot
        .files
        .check_tree(root, ARTIFACTS.into_iter().chain(["release.json"]))?;
    let archive = snapshot
        .files
        .file(&format!("{root}/plugins/secretary-tools.tgz"))?;
    use std::os::unix::fs::FileExt;
    let size = archive
        .metadata()
        .map_err(|_| "release_archive_invalid")?
        .len();
    if size > ARCHIVE_LIMIT {
        return Err("release_archive_invalid");
    }
    let mut bytes = vec![0; size as usize];
    archive
        .read_exact_at(&mut bytes, 0)
        .map_err(|_| "release_archive_invalid")?;
    let hashes = package_hashes(&bytes)?;
    let plugin = plugin_directory
        .to_str()
        .filter(|path| absolute_path(path))
        .ok_or("release_plugin_path")?;
    snapshot.files.check_tree(plugin, PACKAGE_FILES)?;
    for (name, expected) in hashes {
        let path = format!("{plugin}/{name}");
        snapshot.files.open(&path, FileKind::Read)?;
        if snapshot.files.hash(&path)? != expected {
            return Err("release_plugin_mismatch");
        }
    }
    snapshot.files.confirm()
}

fn build_tools(versions: &BTreeMap<String, String>, machine: &str) -> bool {
    let host = format!("{machine}-unknown-linux-gnu");
    let node = versions
        .get("node")
        .and_then(|s| s.strip_prefix('v'))
        .and_then(|s| {
            let parts = s
                .split('.')
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            <[u32; 3]>::try_from(parts).ok()
        });
    versions.len() == 6
        && node.is_some_and(|v| ([24, 22, 1]..[25, 0, 0]).contains(&v) || v >= [26, 12, 1])
        && versions.get("bun").is_some_and(|v| v == "1.2.21")
        && versions.get("rust_host") == Some(&host)
        && versions
            .get("rustc")
            .is_some_and(|v| v.starts_with("rustc 1.95.0 "))
        && versions
            .get("cargo")
            .is_some_and(|v| v.starts_with("cargo 1.95.0 "))
        && versions
            .get("openclaw")
            .is_some_and(|v| v.split_ascii_whitespace().any(|word| word == "2026.10.7"))
}

#[cfg(test)]
mod tests;
