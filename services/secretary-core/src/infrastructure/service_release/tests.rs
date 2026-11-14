use super::*;
use flate2::{write::GzEncoder, Compression};
use std::io::Write;

fn tar_entry(name: &str, body: &[u8], mode: u8) -> Vec<u8> {
    let mut header = [0; 512];
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[124..136].copy_from_slice(format!("{:010o} \0", body.len()).as_bytes());
    header[148..156].fill(b' ');
    header[156] = mode;
    header[257..265].copy_from_slice(b"ustar\x0000");
    let checksum: usize = header.iter().map(|byte| usize::from(*byte)).sum();
    header[148..156].copy_from_slice(format!("{checksum:06o} \0").as_bytes());
    let mut entry = header.to_vec();
    entry.extend(body);
    entry.resize(entry.len().next_multiple_of(512), 0);
    entry
}

fn archive(manifest: &[u8], extra: Option<Vec<u8>>) -> Vec<u8> {
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(&tar_entry("package/", &[], b'5')).unwrap();
    for (name, body) in [
        ("package/package.json", br#"{"name":"@personal-secretary/openclaw-tools","version":"0.1.0","type":"module","license":"MIT","peerDependencies":{"openclaw":"2026.10.7"},"openclaw":{"extensions":["./dist/index.js"]}}"#.as_slice()),
        ("package/openclaw.plugin.json", manifest),
        ("package/dist/index.js", b"export default {};"),
    ] {
        gzip.write_all(&tar_entry(name, body, b'0')).unwrap();
    }
    if let Some(extra) = extra {
        gzip.write_all(&extra).unwrap();
    }
    gzip.write_all(&[0; 1024]).unwrap();
    gzip.finish().unwrap()
}

const MANIFEST: &[u8] =
    include_bytes!("../../../../../plugins/secretary-tools/openclaw.plugin.json");

#[test]
fn pinned_sdk_tar_layout_preserves_the_exact_three_regular_files() {
    // Synthetic payload generated with the locked tar 7.5.22 and the SDK's
    // portable/noMtime/gzip options, without building or activating a plugin.
    let mut decoder = GzDecoder::new(include_bytes!("sdk-ustar-layout.tgz").as_slice());
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes).unwrap();
    let members = package_members(&bytes).unwrap();
    assert_eq!(members.len(), 3);
    assert_eq!(members["package.json"], b"{}\n");
    assert_eq!(members["openclaw.plugin.json"], b"{}\n");
    assert_eq!(members["dist/index.js"], b"export default {};\n");
}

#[test]
fn package_checks_exact_sdk_contract_and_file_digests() {
    let hashes = package_hashes(&archive(MANIFEST, None)).unwrap();
    assert_eq!(hashes["openclaw.plugin.json"], sha256(MANIFEST));
    let mut outdated: Value = serde_json::from_slice(MANIFEST).unwrap();
    outdated["configSchema"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("readOnlyRuntime");
    assert_eq!(
        package_hashes(&archive(outdated.to_string().as_bytes(), None)),
        Err("release_plugin_contract")
    );
}

#[test]
fn duplicate_links_unexpected_paths_and_appended_gzip_data_are_denied() {
    for extra in [
        tar_entry("package/dist/index.js", b"replacement", b'0'),
        tar_entry("package/dist/index.js", &[], b'2'),
        tar_entry("package/other", b"extra", b'0'),
    ] {
        assert_eq!(
            package_hashes(&archive(MANIFEST, Some(extra))),
            Err("release_archive_invalid")
        );
    }
    let mut appended = archive(MANIFEST, None);
    appended.push(0);
    assert_eq!(package_hashes(&appended), Err("release_archive_invalid"));
}

#[test]
fn tool_versions_are_bound_to_target_and_actual_sdk_range() {
    let mut versions = BTreeMap::from([
        ("node".into(), "v24.22.1".into()),
        ("bun".into(), "1.2.21".into()),
        ("rustc".into(), "rustc 1.95.0 (fixture)".into()),
        ("cargo".into(), "cargo 1.95.0 (fixture)".into()),
        ("rust_host".into(), "aarch64-unknown-linux-gnu".into()),
        ("openclaw".into(), "OpenClaw 2026.10.7 (fixture)".into()),
    ]);
    assert!(build_tools(&versions, "aarch64"));
    assert!(!build_tools(&versions, "x86_64"));
    versions.insert("node".into(), "v25.1.0".into());
    assert!(!build_tools(&versions, "aarch64"));
}
