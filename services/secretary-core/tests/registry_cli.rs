//! Real executable + disposable local files; never a substitute registry parser.
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::{Command, Output},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("secretary-registry-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_secretary-core"))
            .args(args)
            .env_clear()
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if child.try_wait().unwrap().is_some() {
                return child.wait_with_output().unwrap();
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("offline CLI did not exit within deadline");
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    fn add(
        &self,
        base: &std::path::Path,
        addition: &std::path::Path,
        output: &std::path::Path,
    ) -> Output {
        self.run(&[
            "registry",
            "add",
            "--config",
            base.to_str().unwrap(),
            "--resource",
            addition.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
    }
    fn config(&self) -> PathBuf {
        let path = self.0.join("registry.json");
        std::fs::write(&path,json!({"version":"PRIVATE_VERSION_CANARY","spreadsheets":[],"documents":[{"alias":"PRIVATE_ALIAS_CANARY","id":"PRIVATE_DOC_ID_CANARY","readable":true,"writable":false}]}).to_string()).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn data(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("expected safe JSON, status={}", output.status))
}
#[test]
fn offline_check_needs_no_runtime_credentials_and_never_discloses_config_values() {
    let f = Fixture::new();
    let path = f.config();
    let before = std::fs::read(&path).unwrap();
    let result = f.run(&["registry", "check", "--config", path.to_str().unwrap()]);
    assert!(result.status.success());
    let body = data(&result);
    assert_eq!(body["ok"], true);
    assert_eq!(body["mode"], "offline");
    assert_eq!(body["summary"]["documents"], 1);
    assert_eq!(body["summary"]["readable_documents"], 1);
    assert_eq!(body["live_access_checked"], false);
    let output = String::from_utf8_lossy(&result.stdout).to_string()
        + &String::from_utf8_lossy(&result.stderr);
    for secret in [
        "PRIVATE_VERSION_CANARY",
        "PRIVATE_ALIAS_CANARY",
        "PRIVATE_DOC_ID_CANARY",
        path.to_str().unwrap(),
    ] {
        assert!(!output.contains(secret));
    }
    assert_eq!(std::fs::read(path).unwrap(), before);
    assert_eq!(std::fs::read_dir(&f.0).unwrap().count(), 1);
}

#[test]
fn init_creates_a_private_inactive_template_and_never_overwrites_existing_files() {
    let f = Fixture::new();
    let path = f.0.join("new.json");
    let result = f.run(&["registry", "init", "--output", path.to_str().unwrap()]);
    assert!(result.status.success());
    let body = data(&result);
    assert_eq!(body["configuration_changed"], true);
    assert_eq!(body["activation_changed"], false);
    assert_eq!(body["summary"]["readable_documents"], 0);
    assert_eq!(body["summary"]["readable_spreadsheets"], 0);
    assert_eq!(
        body["summary"]["configured_write_permissions"],
        json!({"create_tabs":0,"grid_ranges":0,"structured_views":0,"documents":0})
    );
    let bytes = std::fs::read(&path).unwrap();
    let template: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(template["spreadsheets"][0]["readable"], false);
    assert_eq!(template["spreadsheets"][0]["create_tabs"], false);
    assert!(template["spreadsheets"][0]["views"][0]["writable_range"].is_null());
    assert_eq!(template["documents"][0]["readable"], false);
    assert_eq!(template["documents"][0]["writable"], false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(f
        .run(&["registry", "check", "--config", path.to_str().unwrap()])
        .status
        .success());
    let result = f.run(&["registry", "init", "--output", path.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(data(&result)["error"]["code"], "output_exists");
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn syntax_schema_and_domain_errors_are_sanitized_and_share_core_rules() {
    let f = Fixture::new();
    let path = f.config();
    let valid = std::fs::read(&path).unwrap();
    let mut duplicated: Value = serde_json::from_slice(&valid).unwrap();
    let doc = duplicated["documents"][0].clone();
    duplicated["documents"].as_array_mut().unwrap().push(doc);
    let mut range: Value =
        serde_json::from_str(include_str!("../../../config/workspace.example.json")).unwrap();
    range["spreadsheets"][0]["views"][0]["writable_range"] = json!("A1:Z100");
    for (bytes, code) in [
        (b"{PRIVATE_SYNTAX_CANARY".to_vec(), "registry_json_invalid"),
        (
            br#"{"PRIVATE_UNKNOWN_FIELD_CANARY":"PRIVATE_VALUE_CANARY"}"#.to_vec(),
            "registry_schema_invalid",
        ),
        (
            duplicated.to_string().into_bytes(),
            "registry_invalid_document",
        ),
        (
            range.to_string().into_bytes(),
            "registry_invalid_grid_write_range",
        ),
    ] {
        std::fs::write(&path, &bytes).unwrap();
        let result = f.run(&["registry", "check", "--config", path.to_str().unwrap()]);
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(data(&result)["error"]["code"], code);
        let output = String::from_utf8_lossy(&result.stdout).to_string()
            + &String::from_utf8_lossy(&result.stderr);
        assert!(!output.contains("CANARY"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn permission_summary_describes_configured_grants_without_activating_anything() {
    let f = Fixture::new();
    let path = f.0.join("registry.json");
    let mut registry: Value =
        serde_json::from_str(include_str!("../../../config/workspace.example.json")).unwrap();
    registry["spreadsheets"][0]["create_tabs"] = json!(true);
    registry["spreadsheets"][0]["views"][0]["writable_range"] = json!("B2:C8");
    registry["documents"][0]["writable"] = json!(true);
    let bytes = registry.to_string();
    std::fs::write(&path, &bytes).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_secretary-core"))
        .args(["registry", "check", "--config", path.to_str().unwrap()])
        .env_clear()
        .env("TOKIO_WORKER_THREADS", "invalid")
        .env("SECRETARY_CONFIG", "PRIVATE_ENV_CANARY")
        .env("SECRETARY_WRITES_ENABLED", "true")
        .env("SECRETARY_STATE_DIR", f.0.join("must-not-exist"))
        .env(
            "GOOGLE_APPLICATION_CREDENTIALS",
            "PRIVATE_CREDENTIAL_CANARY",
        )
        .output()
        .unwrap();
    assert!(output.status.success());
    let body = data(&output);
    assert_eq!(
        body["summary"]["configured_write_permissions"],
        json!({"create_tabs":1,"grid_ranges":1,"structured_views":0,"documents":1})
    );
    assert_eq!(body["live_access_checked"], false);
    assert_eq!(body["activation_changed"], false);
    assert_eq!(body["configuration_changed"], false);
    assert!(!f.0.join("must-not-exist").exists());
    assert_eq!(std::fs::read_to_string(path).unwrap(), bytes);
}

#[test]
fn unsafe_or_oversized_inputs_fail_without_dumping_paths_or_content() {
    let f = Fixture::new();
    let missing = f.0.join("PRIVATE_MISSING_PATH_CANARY");
    for (path, code) in [
        (&f.0, "registry_unsafe_file"),
        (&missing, "registry_unavailable"),
    ] {
        let result = f.run(&["registry", "check", "--config", path.to_str().unwrap()]);
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(data(&result)["error"]["code"], code);
        assert!(!String::from_utf8_lossy(&result.stdout).contains("CANARY"));
    }
    #[cfg(unix)]
    {
        let fifo = f.0.join("fifo");
        assert!(Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success());
        let result = f.run(&["registry", "check", "--config", fifo.to_str().unwrap()]);
        assert_eq!(data(&result)["error"]["code"], "registry_unsafe_file");
    }
    let path = f.0.join("large.json");
    std::fs::write(&path, vec![b'x'; 1024 * 1024 + 1]).unwrap();
    let result = f.run(&["registry", "check", "--config", path.to_str().unwrap()]);
    assert_eq!(data(&result)["error"]["code"], "registry_too_large");
    assert!(result.stdout.len() < 512);
}

#[cfg(unix)]
#[test]
fn final_symlinks_are_neither_read_nor_replaced() {
    let f = Fixture::new();
    let target = f.config();
    let original = std::fs::read(&target).unwrap();
    let link = f.0.join("link.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let checked = f.run(&["registry", "check", "--config", link.to_str().unwrap()]);
    assert_eq!(data(&checked)["error"]["code"], "registry_unsafe_file");
    let created = f.run(&["registry", "init", "--output", link.to_str().unwrap()]);
    assert_eq!(data(&created)["error"]["code"], "output_exists");
    assert!(std::fs::symlink_metadata(link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read(target).unwrap(), original);
}

#[test]
fn unknown_or_incomplete_arguments_return_safe_usage_without_starting_service() {
    let f = Fixture::new();
    for args in [
        vec!["registry"],
        vec!["registry", "check"],
        vec!["registry", "init", "--config", "PRIVATE_ARGUMENT_CANARY"],
        vec!["PRIVATE_ARGUMENT_CANARY"],
    ] {
        let result = f.run(&args);
        assert_eq!(result.status.code(), Some(2));
        assert_eq!(data(&result)["error"]["code"], "usage");
        assert!(!String::from_utf8_lossy(&result.stdout).contains("CANARY"));
    }
    assert_eq!(std::fs::read_dir(&f.0).unwrap().count(), 0);
}

#[test]
fn add_document_creates_an_inactive_candidate_preserving_base_and_private_inputs() {
    let f = Fixture::new();
    let base = f.config();
    let before = std::fs::read(&base).unwrap();
    let addition = f.0.join("PRIVATE_ADDITION_PATH_CANARY.json");
    let intent = json!({"version":"PRIVATE_NEW_VERSION_CANARY","resource":{"kind":"document","alias":"PRIVATE_NEW_ALIAS_CANARY","id":"PRIVATE_NEW_ID_CANARY"}}).to_string();
    std::fs::write(&addition, &intent).unwrap();
    let candidate = f.0.join("candidate.json");
    let result = f.run(&[
        "registry",
        "add",
        "--config",
        base.to_str().unwrap(),
        "--resource",
        addition.to_str().unwrap(),
        "--output",
        candidate.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", data(&result));
    let body = data(&result);
    assert_eq!(body["summary"]["documents"], 2);
    assert_eq!(body["summary"]["readable_documents"], 1);
    assert_eq!(body["activation_changed"], false);
    assert_eq!(body["live_access_checked"], false);
    assert_eq!(body["configuration_changed"], true);
    assert!(result.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&result.stdout).contains("PRIVATE"));
    let draft: Value = serde_json::from_slice(&std::fs::read(&candidate).unwrap()).unwrap();
    assert_eq!(draft["version"], "PRIVATE_NEW_VERSION_CANARY");
    assert_eq!(
        draft["documents"][0],
        serde_json::from_slice::<Value>(&before).unwrap()["documents"][0]
    );
    assert_eq!(
        draft["documents"][1],
        json!({"alias":"PRIVATE_NEW_ALIAS_CANARY","id":"PRIVATE_NEW_ID_CANARY","readable":false,"writable":false})
    );
    assert_eq!(std::fs::read(base).unwrap(), before);
    assert_eq!(std::fs::read_to_string(addition).unwrap(), intent);
    assert!(f
        .run(&["registry", "check", "--config", candidate.to_str().unwrap()])
        .status
        .success());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(candidate).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn add_spreadsheet_builds_bounded_inactive_grids_without_domain_boilerplate() {
    let f = Fixture::new();
    let base = f.0.join("base.json");
    let mut original: Value =
        serde_json::from_str(include_str!("../../../config/workspace.example.json")).unwrap();
    original["spreadsheets"][0]["create_tabs"] = json!(true);
    original["spreadsheets"][0]["views"][0]["writable_range"] = json!("B2:C8");
    original["documents"][0]["writable"] = json!(true);
    std::fs::write(&base, original.to_string()).unwrap();
    let addition = f.0.join("addition.json");
    std::fs::write(&addition, json!({"version":"v2","resource":{"kind":"spreadsheet","alias":"inventory","id":"inventory-id","views":[{"name":"stock","sheet_id":0,"title":"Stock","range":"A1:D20"},{"name":"summary","sheet_id":8,"title":"Summary","range":"B2:B2"}]}}).to_string()).unwrap();
    let candidate = f.0.join("candidate.json");
    let result = f.run(&[
        "registry",
        "add",
        "--config",
        base.to_str().unwrap(),
        "--resource",
        addition.to_str().unwrap(),
        "--output",
        candidate.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", data(&result));
    let body = data(&result);
    assert_eq!(body["summary"]["spreadsheets"], 2);
    assert_eq!(body["summary"]["views"], 3);
    assert_eq!(body["summary"]["readable_spreadsheets"], 1);
    assert_eq!(
        body["summary"]["configured_write_permissions"],
        json!({"create_tabs":1,"grid_ranges":1,"structured_views":0,"documents":1})
    );
    let draft: Value = serde_json::from_slice(&std::fs::read(&candidate).unwrap()).unwrap();
    assert_eq!(draft["spreadsheets"][0], original["spreadsheets"][0]);
    assert_eq!(draft["documents"], original["documents"]);
    let added = &draft["spreadsheets"][1];
    assert_eq!(added["readable"], false);
    assert_eq!(added["create_tabs"], false);
    assert_eq!(
        added["views"][0],
        json!({"name":"stock","sheet_id":0,"title":"Stock","range":"A1:D20","kind":"grid","headers":{},"merges":[],"formulas":{}})
    );
    assert_eq!(added["views"][1]["sheet_id"], 8);
    assert!(f
        .run(&["registry", "check", "--config", candidate.to_str().unwrap()])
        .status
        .success());
}

#[test]
fn add_rejects_collisions_versions_permissions_and_invalid_grids_before_output_creation() {
    let f = Fixture::new();
    let base = f.config();
    let before = std::fs::read(&base).unwrap();
    let addition = f.0.join("addition.json");
    let candidate = f.0.join("candidate.json");
    let doc = json!({"version":"v2","resource":{"kind":"document","alias":"new","id":"new-id"}});
    let sheet = json!({"version":"v2","resource":{"kind":"spreadsheet","alias":"new","id":"new-id","views":[{"name":"grid","sheet_id":0,"title":"Sheet1","range":"A1:B10"}]}});
    let mut cases = vec![];
    for (field, value, code) in [
        (
            "alias",
            json!("PRIVATE_ALIAS_CANARY"),
            "registry_invalid_document",
        ),
        (
            "id",
            json!("PRIVATE_DOC_ID_CANARY"),
            "registry_invalid_document",
        ),
        (
            "id",
            json!("https://PRIVATE_ID_CANARY"),
            "registry_invalid_document",
        ),
        ("readable", json!(true), "registry_schema_invalid"),
        ("writable", json!(true), "registry_schema_invalid"),
        (
            "PRIVATE_UNKNOWN_FIELD_CANARY",
            json!("PRIVATE_VALUE_CANARY"),
            "registry_schema_invalid",
        ),
    ] {
        let mut changed = doc.clone();
        changed["resource"][field] = value;
        cases.push((changed, code));
    }
    for version in ["", "   ", "PRIVATE_VERSION_CANARY"] {
        let mut changed = doc.clone();
        changed["version"] = json!(version);
        cases.push((
            changed,
            if version == "PRIVATE_VERSION_CANARY" {
                "registry_version_unchanged"
            } else {
                "registry_bounds"
            },
        ));
    }
    for (field, value, code) in [
        ("range", json!("A1:Z10000"), "registry_invalid_grid_range"),
        ("sheet_id", json!(-1), "registry_invalid_view"),
        ("title", json!(""), "registry_invalid_view"),
        ("name", json!(""), "registry_invalid_view"),
        ("writable_range", json!("A1:A1"), "registry_schema_invalid"),
        ("kind", json!("structured_tasks"), "registry_schema_invalid"),
    ] {
        let mut changed = sheet.clone();
        changed["resource"]["views"][0][field] = value;
        cases.push((changed, code));
    }
    let mut duplicate = sheet.clone();
    duplicate["resource"]["views"]
        .as_array_mut()
        .unwrap()
        .push(sheet["resource"]["views"][0].clone());
    cases.push((duplicate, "registry_invalid_view"));
    for views in [
        json!([]),
        json!(vec![sheet["resource"]["views"][0].clone(); 11]),
    ] {
        let mut changed = sheet.clone();
        changed["resource"]["views"] = views;
        cases.push((changed, "registry_invalid_spreadsheet"));
    }
    // Alias and ID uniqueness are shared across Sheets and Docs.
    for (field, value) in [
        ("alias", "PRIVATE_ALIAS_CANARY"),
        ("id", "PRIVATE_DOC_ID_CANARY"),
    ] {
        let mut changed = sheet.clone();
        changed["resource"][field] = json!(value);
        cases.push((changed, "registry_invalid_spreadsheet"));
    }
    let mut changed = sheet.clone();
    changed["resource"]["create_tabs"] = json!(true);
    cases.push((changed, "registry_schema_invalid"));
    for (intent, code) in cases {
        let bytes = intent.to_string();
        std::fs::write(&addition, &bytes).unwrap();
        let result = f.add(&base, &addition, &candidate);
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(data(&result)["error"]["code"], code);
        assert!(result.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&result.stdout).contains("CANARY"));
        assert!(!candidate.exists());
        assert_eq!(std::fs::read(&base).unwrap(), before);
        assert_eq!(std::fs::read_to_string(&addition).unwrap(), bytes);
    }
}

#[test]
fn add_replay_and_output_aliases_cannot_overwrite_sources_or_existing_candidates() {
    let f = Fixture::new();
    let base = f.config();
    let before = std::fs::read(&base).unwrap();
    let addition = f.0.join("addition.json");
    let intent = json!({"version":"v2","resource":{"kind":"document","alias":"new","id":"new-id"}})
        .to_string();
    std::fs::write(&addition, &intent).unwrap();
    let candidate = f.0.join("candidate.json");
    assert!(f.add(&base, &addition, &candidate).status.success());
    let bytes = std::fs::read(&candidate).unwrap();
    for output in [&candidate, &base, &addition] {
        let result = f.add(&base, &addition, output);
        assert_eq!(data(&result)["error"]["code"], "output_exists");
    }
    #[cfg(unix)]
    {
        let link = f.0.join("link.json");
        std::os::unix::fs::symlink(&base, &link).unwrap();
        assert_eq!(
            data(&f.add(&base, &addition, &link))["error"]["code"],
            "output_exists"
        );
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
    }
    assert_eq!(std::fs::read(&base).unwrap(), before);
    assert_eq!(std::fs::read_to_string(&addition).unwrap(), intent);
    assert_eq!(std::fs::read(&candidate).unwrap(), bytes);
    // Reapplying the same resource to the candidate must not silently duplicate it.
    let mut repeated: Value = serde_json::from_str(&intent).unwrap();
    repeated["version"] = json!("v3");
    std::fs::write(&addition, repeated.to_string()).unwrap();
    let next = f.0.join("next.json");
    assert_eq!(
        data(&f.add(&candidate, &addition, &next))["error"]["code"],
        "registry_invalid_document"
    );
    assert!(!next.exists());
}

#[test]
fn addition_inputs_and_complete_candidates_remain_bounded_and_sanitized() {
    let f = Fixture::new();
    let base = f.config();
    let addition = f.0.join("PRIVATE_INPUT_CANARY");
    let candidate = f.0.join("candidate.json");
    for (bytes, code) in [
        (b"{PRIVATE_SYNTAX_CANARY".to_vec(), "registry_json_invalid"),
        (
            br#"{"version":"v2","version":"PRIVATE_DUPLICATE_CANARY","resource":{}}"#.to_vec(),
            "registry_schema_invalid",
        ),
        (vec![b'x'; 1024 * 1024 + 1], "registry_too_large"),
    ] {
        std::fs::write(&addition, bytes).unwrap();
        let result = f.add(&base, &addition, &candidate);
        assert_eq!(data(&result)["error"]["code"], code);
        assert!(result.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&result.stdout).contains("CANARY"));
        assert!(!candidate.exists());
    }
    std::fs::remove_file(&addition).unwrap();
    std::fs::create_dir(&addition).unwrap();
    assert_eq!(
        data(&f.add(&base, &addition, &candidate))["error"]["code"],
        "registry_unsafe_file"
    );
    std::fs::remove_dir(&addition).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&base, &addition).unwrap();
        assert_eq!(
            data(&f.add(&base, &addition, &candidate))["error"]["code"],
            "registry_unsafe_file"
        );
        std::fs::remove_file(&addition).unwrap();
        assert!(Command::new("mkfifo")
            .arg(&addition)
            .status()
            .unwrap()
            .success());
        assert_eq!(
            data(&f.add(&base, &addition, &candidate))["error"]["code"],
            "registry_unsafe_file"
        );
        std::fs::remove_file(&addition).unwrap();
    }
    let documents: Vec<_> = (0..20)
        .map(|n| json!({"alias":format!("doc-{n}"),"id":format!("id-{n}"),"readable":false}))
        .collect();
    std::fs::write(
        &base,
        json!({"version":"v1","documents":documents}).to_string(),
    )
    .unwrap();
    let intent = json!({"version":"v2","resource":{"kind":"document","alias":"new","id":"new-id"}});
    std::fs::write(&addition, intent.to_string()).unwrap();
    assert_eq!(
        data(&f.add(&base, &addition, &candidate))["error"]["code"],
        "registry_bounds"
    );
    let mut bounded: Value = serde_json::from_slice(&std::fs::read(&base).unwrap()).unwrap();
    bounded["documents"].as_array_mut().unwrap().pop();
    std::fs::write(&base, bounded.to_string()).unwrap();
    let mut huge = intent;
    huge["version"] = json!("v".repeat(1024 * 1024 - 1000));
    let bytes = huge.to_string();
    assert!(bytes.len() < 1024 * 1024);
    std::fs::write(&addition, bytes).unwrap();
    assert_eq!(
        data(&f.add(&base, &addition, &candidate))["error"]["code"],
        "registry_too_large"
    );
    assert!(!candidate.exists());
}

#[test]
fn add_grid_view_registers_a_new_tab_without_changing_parent_permissions_or_old_views() {
    for readable in [true, false] {
        let f = Fixture::new();
        let base = f.0.join("base.json");
        let mut original: Value =
            serde_json::from_str(include_str!("../../../config/workspace.example.json")).unwrap();
        original["spreadsheets"][0]["readable"] = json!(readable);
        original["spreadsheets"][0]["create_tabs"] = json!(readable);
        if readable {
            original["spreadsheets"][0]["views"][0]["writable_range"] = json!("B2:C8");
        }
        let before = original.to_string();
        std::fs::write(&base, &before).unwrap();
        let addition = f.0.join("PRIVATE_ADDITION_CANARY.json");
        let intent = json!({"version":"PRIVATE_NEW_VERSION_CANARY","resource":{"kind":"grid_view","spreadsheet":"operations","view":{"name":"PRIVATE_VIEW_CANARY","sheet_id":8,"title":"PRIVATE_TITLE_CANARY","range":"A1:D20"}}}).to_string();
        std::fs::write(&addition, &intent).unwrap();
        let candidate = f.0.join("candidate.json");
        let result = f.add(&base, &addition, &candidate);
        assert!(result.status.success(), "{}", data(&result));
        let body = data(&result);
        assert_eq!(body["summary"]["spreadsheets"], 1);
        assert_eq!(body["summary"]["views"], 2);
        assert_eq!(
            body["summary"]["readable_spreadsheets"],
            if readable { 1 } else { 0 }
        );
        assert_eq!(
            body["summary"]["configured_write_permissions"]["grid_ranges"],
            if readable { 1 } else { 0 }
        );
        assert_eq!(body["activation_changed"], false);
        assert_eq!(body["live_access_checked"], false);
        assert!(result.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&result.stdout).contains("CANARY"));
        let draft: Value = serde_json::from_slice(&std::fs::read(&candidate).unwrap()).unwrap();
        let mut expected = original;
        expected["version"] = json!("PRIVATE_NEW_VERSION_CANARY");
        expected["spreadsheets"][0]["views"].as_array_mut().unwrap().push(json!({"name":"PRIVATE_VIEW_CANARY","sheet_id":8,"title":"PRIVATE_TITLE_CANARY","range":"A1:D20","kind":"grid","headers":{},"merges":[],"formulas":{}}));
        assert_eq!(draft, expected);
        assert_eq!(std::fs::read_to_string(&base).unwrap(), before);
        assert_eq!(std::fs::read_to_string(&addition).unwrap(), intent);
        assert!(f
            .run(&["registry", "check", "--config", candidate.to_str().unwrap()])
            .status
            .success());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(candidate).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}

#[test]
fn add_grid_view_rejects_missing_parents_and_conflicting_tab_mappings_without_creating_output() {
    let f = Fixture::new();
    let base = f.0.join("base.json");
    let before = include_str!("../../../config/workspace.example.json");
    std::fs::write(&base, before).unwrap();
    let addition = f.0.join("addition.json");
    let candidate = f.0.join("candidate.json");
    for (alias, id, title, code) in [
        (
            "PRIVATE_MISSING_CANARY",
            8,
            "Stock",
            "registry_spreadsheet_not_found",
        ),
        ("notes", 8, "Stock", "registry_spreadsheet_not_found"),
        (
            "operations",
            7,
            "PRIVATE_CHANGED_TITLE_CANARY",
            "registry_view_identity_conflict",
        ),
        (
            "operations",
            8,
            "Operations",
            "registry_view_identity_conflict",
        ),
        (
            "operations",
            8,
            "operations",
            "registry_view_identity_conflict",
        ),
    ] {
        let intent = json!({"version":"v2","resource":{"kind":"grid_view","spreadsheet":alias,"view":{"name":"new","sheet_id":id,"title":title,"range":"A1:D20"}}}).to_string();
        std::fs::write(&addition, &intent).unwrap();
        let result = f.add(&base, &addition, &candidate);
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(data(&result)["error"]["code"], code);
        assert!(!candidate.exists());
        assert_eq!(std::fs::read_to_string(&base).unwrap(), before);
        assert_eq!(std::fs::read_to_string(&addition).unwrap(), intent);
        assert!(result.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&result.stdout).contains("CANARY"));
    }
}

#[test]
fn add_grid_view_accepts_another_range_on_the_same_tab_and_refuses_replay_or_overwrite() {
    let f = Fixture::new();
    let base = f.0.join("base.json");
    let before = include_str!("../../../config/workspace.example.json");
    std::fs::write(&base, before).unwrap();
    let addition = f.0.join("addition.json");
    let mut intent = json!({"version":"v2","resource":{"kind":"grid_view","spreadsheet":"operations","view":{"name":"second-range","sheet_id":7,"title":"Operations","range":"A1:A2"}}});
    std::fs::write(&addition, intent.to_string()).unwrap();
    let candidate = f.0.join("candidate.json");
    assert!(f.add(&base, &addition, &candidate).status.success());
    let bytes = std::fs::read(&candidate).unwrap();
    let draft: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(draft["spreadsheets"][0]["views"][1]["range"], "A1:A2");
    assert_eq!(draft["spreadsheets"][0]["views"][1]["sheet_id"], 7);
    assert_eq!(
        data(&f.add(&base, &addition, &candidate))["error"]["code"],
        "output_exists"
    );
    for path in [&base, &addition] {
        assert_eq!(
            data(&f.add(&base, &addition, path))["error"]["code"],
            "output_exists"
        );
    }
    #[cfg(unix)]
    {
        let link = f.0.join("link.json");
        std::os::unix::fs::symlink(&base, &link).unwrap();
        assert_eq!(
            data(&f.add(&base, &addition, &link))["error"]["code"],
            "output_exists"
        );
        assert!(std::fs::symlink_metadata(link)
            .unwrap()
            .file_type()
            .is_symlink());
    }
    assert_eq!(std::fs::read(&candidate).unwrap(), bytes);
    assert_eq!(std::fs::read_to_string(&base).unwrap(), before);
    intent["version"] = json!("v3");
    std::fs::write(&addition, intent.to_string()).unwrap();
    let next = f.0.join("next.json");
    assert_eq!(
        data(&f.add(&candidate, &addition, &next))["error"]["code"],
        "registry_invalid_view"
    );
    assert!(!next.exists());
}

#[test]
fn add_grid_view_obeys_shared_view_bounds_and_cannot_inject_permissions_or_provider_fields() {
    let f = Fixture::new();
    let base = f.0.join("base.json");
    let original: Value =
        serde_json::from_str(include_str!("../../../config/workspace.example.json")).unwrap();
    std::fs::write(&base, original.to_string()).unwrap();
    let addition = f.0.join("addition.json");
    let candidate = f.0.join("candidate.json");
    let valid = json!({"version":"v2","resource":{"kind":"grid_view","spreadsheet":"operations","view":{"name":"second","sheet_id":7,"title":"Operations","range":"A1:A2"}}});
    for (field, value, code) in [
        ("name", json!("overview"), "registry_invalid_view"),
        ("sheet_id", json!(-1), "registry_invalid_view"),
        ("range", json!("A1:Z10000"), "registry_invalid_grid_range"),
        ("writable_range", json!("A1:A2"), "registry_schema_invalid"),
        (
            "write_schema",
            json!({"PRIVATE_CANARY":true}),
            "registry_schema_invalid",
        ),
        ("readable", json!(true), "registry_schema_invalid"),
        ("kind", json!("structured_tasks"), "registry_schema_invalid"),
        (
            "sheet_id",
            json!("PRIVATE_CANARY"),
            "registry_schema_invalid",
        ),
    ] {
        let mut intent = valid.clone();
        intent["resource"]["view"][field] = value;
        // Use a distinct title for invalid negative ID to isolate the Core ID rule.
        if field == "sheet_id" && intent["resource"]["view"]["sheet_id"] == -1 {
            intent["resource"]["view"]["title"] = json!("New");
        }
        std::fs::write(&addition, intent.to_string()).unwrap();
        let result = f.add(&base, &addition, &candidate);
        assert_eq!(data(&result)["error"]["code"], code);
        assert!(!candidate.exists());
        assert!(result.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&result.stdout).contains("CANARY"));
    }
    let mut full = original;
    for n in 1..10 {
        let mut view = full["spreadsheets"][0]["views"][0].clone();
        view["name"] = json!(format!("old-{n}"));
        full["spreadsheets"][0]["views"]
            .as_array_mut()
            .unwrap()
            .push(view);
    }
    let before = full.to_string();
    std::fs::write(&base, &before).unwrap();
    std::fs::write(&addition, valid.to_string()).unwrap();
    assert_eq!(
        data(&f.add(&base, &addition, &candidate))["error"]["code"],
        "registry_invalid_spreadsheet"
    );
    assert!(!candidate.exists());
    assert_eq!(std::fs::read_to_string(&base).unwrap(), before);
}

impl Fixture {
    fn diff(
        &self,
        before: &std::path::Path,
        after: &std::path::Path,
        report: &std::path::Path,
    ) -> Output {
        self.run(&[
            "registry",
            "diff",
            "--before",
            before.to_str().unwrap(),
            "--after",
            after.to_str().unwrap(),
            "--output",
            report.to_str().unwrap(),
        ])
    }
}

#[test]
fn diff_creates_private_review_of_permission_target_and_range_changes_without_disclosing_values() {
    let f = Fixture::new();
    let before = f.0.join("PRIVATE_BEFORE_CANARY.json");
    let original: Value =
        serde_json::from_str(include_str!("../../../config/workspace.example.json")).unwrap();
    std::fs::write(&before, original.to_string()).unwrap();
    let after = f.0.join("after.json");
    let mut candidate = original.clone();
    candidate["version"] = json!("PRIVATE_VERSION_CANARY");
    candidate["documents"][0]["id"] = json!("PRIVATE_ID_CANARY");
    candidate["documents"][0]["writable"] = json!(true);
    candidate["spreadsheets"][0]["views"][0]["writable_range"] = json!("B2:C8");
    let mut new_view = candidate["spreadsheets"][0]["views"][0].clone();
    new_view["name"] = json!("PRIVATE_VIEW_CANARY");
    new_view["sheet_id"] = json!(8);
    new_view["title"] = json!("PRIVATE_TITLE_CANARY");
    new_view["range"] = json!("A1:D20");
    new_view.as_object_mut().unwrap().remove("writable_range");
    candidate["spreadsheets"][0]["views"]
        .as_array_mut()
        .unwrap()
        .push(new_view);
    std::fs::write(&after, candidate.to_string()).unwrap();
    let report = f.0.join("PRIVATE_REPORT_CANARY.json");
    let result = f.diff(&before, &after, &report);
    assert!(result.status.success(), "{}", data(&result));
    let body = data(&result);
    assert_eq!(
        body["summary"],
        json!({"spreadsheets":{"added":0,"removed":0,"modified":1},"documents":{"added":0,"removed":0,"modified":1},"views":{"added":1,"removed":0,"modified":1},"version_changed":true,"mappings_changed":true,"version_change_required":false})
    );
    assert_eq!(body["configuration_changed"], false);
    assert_eq!(body["report_created"], true);
    assert_eq!(body["activation_changed"], false);
    assert_eq!(body["live_access_checked"], false);
    assert!(result.stderr.is_empty());
    let output = String::from_utf8_lossy(&result.stdout);
    for private in [
        "CANARY",
        "operations",
        "notes",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
        report.to_str().unwrap(),
    ] {
        assert!(!output.contains(private));
    }
    let details: Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    assert_eq!(details["format_version"], 1);
    assert_eq!(details["summary"], body["summary"]);
    assert_eq!(details["changes"]["documents"][0]["key"], "notes");
    assert_eq!(details["changes"]["documents"][0]["change"], "modified");
    assert_eq!(
        details["changes"]["documents"][0]["before"],
        original["documents"][0]
    );
    assert_eq!(
        details["changes"]["documents"][0]["after"],
        candidate["documents"][0]
    );
    assert_eq!(
        details["changes"]["spreadsheets"][0]["after"]["views"][0]["name"],
        "PRIVATE_VIEW_CANARY"
    );
    assert_eq!(
        details["changes"]["spreadsheets"][0]["after"]["views"][1]["writable_range"],
        "B2:C8"
    );
    assert_eq!(
        std::fs::read_to_string(&before).unwrap(),
        original.to_string()
    );
    assert_eq!(
        std::fs::read_to_string(&after).unwrap(),
        candidate.to_string()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(report).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn diff_ignores_named_list_order_and_defaults_but_flags_same_version_mapping_changes() {
    let f = Fixture::new();
    let before = f.0.join("before.json");
    let after = f.0.join("after.json");
    let mut original: Value =
        serde_json::from_str(include_str!("../../../config/workspace.example.json")).unwrap();
    let mut second_view = original["spreadsheets"][0]["views"][0].clone();
    second_view["name"] = json!("other");
    original["spreadsheets"][0]["views"]
        .as_array_mut()
        .unwrap()
        .push(second_view);
    let mut second_sheet = original["spreadsheets"][0].clone();
    second_sheet["alias"] = json!("second");
    second_sheet["id"] = json!("second-id");
    original["spreadsheets"]
        .as_array_mut()
        .unwrap()
        .push(second_sheet);
    original["documents"]
        .as_array_mut()
        .unwrap()
        .push(json!({"alias":"second-doc","id":"second-doc-id","readable":false,"writable":false}));
    let bytes = original.to_string();
    std::fs::write(&before, &bytes).unwrap();
    let mut reordered = original.clone();
    reordered["spreadsheets"].as_array_mut().unwrap().reverse();
    reordered["documents"].as_array_mut().unwrap().reverse();
    for s in reordered["spreadsheets"].as_array_mut().unwrap() {
        s.as_object_mut().unwrap().remove("create_tabs");
        s["views"].as_array_mut().unwrap().reverse();
    }
    for d in reordered["documents"].as_array_mut().unwrap() {
        d.as_object_mut().unwrap().remove("writable");
    }
    for (n, version, mappings, version_changed, required) in [
        (0, "generic-workspace-v1", false, false, false),
        (1, "v2", false, true, false),
        (2, "generic-workspace-v1", true, false, true),
    ] {
        let mut candidate = reordered.clone();
        candidate["version"] = json!(version);
        if mappings {
            candidate["spreadsheets"][0]["views"][0]["range"] = json!("A1:C3");
        }
        std::fs::write(&after, candidate.to_string()).unwrap();
        let report = f.0.join(format!("report-{n}.json"));
        let result = f.diff(&before, &after, &report);
        assert!(result.status.success(), "{}", data(&result));
        let body = data(&result);
        assert_eq!(body["summary"]["mappings_changed"], mappings);
        assert_eq!(body["summary"]["version_changed"], version_changed);
        assert_eq!(body["summary"]["version_change_required"], required);
        assert_eq!(
            body["summary"]["views"],
            json!({"added":0,"removed":0,"modified":if mappings {1} else {0}})
        );
        if !mappings {
            let details: Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
            assert_eq!(
                details["changes"],
                json!({"spreadsheets":[],"documents":[]})
            );
        }
    }
    assert_eq!(std::fs::read_to_string(before).unwrap(), bytes);
}

#[test]
fn diff_treats_alias_renames_as_removals_and_additions_without_guessing_identity() {
    let f = Fixture::new();
    let before = f.0.join("before.json");
    let after = f.0.join("after.json");
    let original: Value =
        serde_json::from_str(include_str!("../../../config/workspace.example.json")).unwrap();
    std::fs::write(&before, original.to_string()).unwrap();
    let mut candidate = original.clone();
    candidate["version"] = json!("v2");
    candidate["spreadsheets"][0]["alias"] = json!("renamed-sheet");
    candidate["documents"][0]["alias"] = json!("renamed-doc");
    std::fs::write(&after, candidate.to_string()).unwrap();
    let report = f.0.join("report.json");
    let result = f.diff(&before, &after, &report);
    assert!(result.status.success(), "{}", data(&result));
    let body = data(&result);
    for kind in ["spreadsheets", "documents", "views"] {
        assert_eq!(
            body["summary"][kind],
            json!({"added":1,"removed":1,"modified":0})
        );
    }
    let details: Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    let docs = details["changes"]["documents"].as_array().unwrap();
    assert_eq!(docs[0]["key"], "notes");
    assert_eq!(docs[0]["change"], "removed");
    assert!(docs[0]["after"].is_null());
    assert_eq!(docs[1]["key"], "renamed-doc");
    assert_eq!(docs[1]["change"], "added");
    assert!(docs[1]["before"].is_null());
    assert_eq!(docs[1]["after"]["id"], docs[0]["before"]["id"]);
}

#[test]
fn diff_rejects_invalid_inputs_and_output_replay_aliases_without_overwriting_anything() {
    let f = Fixture::new();
    let before = f.config();
    let bytes = std::fs::read(&before).unwrap();
    let after = f.0.join("after.json");
    let report = f.0.join("report.json");
    std::fs::write(&after, b"{PRIVATE_INVALID_CANARY").unwrap();
    for (left, right) in [(&before, &after), (&after, &before)] {
        let result = f.diff(left, right, &report);
        assert_eq!(data(&result)["error"]["code"], "registry_json_invalid");
        assert!(!report.exists());
        assert!(result.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&result.stdout).contains("CANARY"));
    }
    std::fs::write(&after, &bytes).unwrap();
    assert!(f.diff(&before, &after, &report).status.success());
    let report_bytes = std::fs::read(&report).unwrap();
    for output in [&before, &after, &report] {
        assert_eq!(
            data(&f.diff(&before, &after, output))["error"]["code"],
            "output_exists"
        );
    }
    #[cfg(unix)]
    {
        let link = f.0.join("link");
        std::os::unix::fs::symlink(&before, &link).unwrap();
        assert_eq!(
            data(&f.diff(&before, &after, &link))["error"]["code"],
            "output_exists"
        );
        let next = f.0.join("next.json");
        for (left, right) in [(&link, &after), (&before, &link)] {
            assert_eq!(
                data(&f.diff(left, right, &next))["error"]["code"],
                "registry_unsafe_file"
            );
            assert!(!next.exists());
        }
        assert!(std::fs::symlink_metadata(link)
            .unwrap()
            .file_type()
            .is_symlink());
    }
    assert_eq!(std::fs::read(&before).unwrap(), bytes);
    assert_eq!(std::fs::read(&after).unwrap(), bytes);
    assert_eq!(std::fs::read(&report).unwrap(), report_bytes);
}

#[test]
fn diff_caps_complete_private_report_and_never_truncates_or_creates_an_unusable_report() {
    let f = Fixture::new();
    let before = f.0.join("before.json");
    let after = f.0.join("after.json");
    let report = f.0.join("report.json");
    let registry =
        json!({"version":"v".repeat(600_000),"spreadsheets":[],"documents":[]}).to_string();
    assert!(registry.len() < 1024 * 1024);
    std::fs::write(&before, &registry).unwrap();
    std::fs::write(&after, &registry).unwrap();
    let result = f.diff(&before, &after, &report);
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(data(&result)["error"]["code"], "registry_too_large");
    assert!(!report.exists());
    assert!(result.stderr.is_empty());
    assert!(result.stdout.len() < 512);
    assert_eq!(std::fs::read_to_string(before).unwrap(), registry);
    assert_eq!(std::fs::read_to_string(after).unwrap(), registry);
}
