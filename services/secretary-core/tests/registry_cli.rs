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
