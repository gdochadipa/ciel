//! Executable boundary for offline service checks; fixtures never contain secrets.
use secretary_core::domain::service_config::{Manifest, ServicePolicy};
use serde_json::{json, Value};
use std::{
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const EXAMPLE: &str = include_str!("../../../config/readonly-manifest.example.json");

fn run(args: &[&str], environment: &[(&str, &str)]) -> Output {
    let child = Command::new(env!("CARGO_BIN_EXE_secretary-core"))
        .args(args)
        .env_clear()
        .envs(environment.iter().copied())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start isolated service CLI");
    wait(child)
}

fn wait(mut child: Child) -> Output {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().expect("poll service CLI").is_some() {
            return child
                .wait_with_output()
                .expect("collect service CLI output");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("offline service CLI did not exit within deadline");
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn output_text(result: &Output) -> String {
    String::from_utf8_lossy(&result.stdout).to_string() + &String::from_utf8_lossy(&result.stderr)
}

fn body(result: &Output) -> Value {
    serde_json::from_slice(&result.stdout)
        .unwrap_or_else(|_| panic!("expected sanitized JSON, status={}", result.status))
}

fn check(path: &str, environment: &[(&str, &str)]) -> Output {
    run(&["service", "check", "--manifest", path], environment)
}

#[test]
fn service_usage_and_unknown_commands_fail_closed_without_startup_fallback() {
    for args in [
        vec!["service"],
        vec!["service", "unknown"],
        vec!["service", "serve-readonly", "--manifest", "/tmp/candidate"],
        vec!["service", "check", "--manifest"],
        vec!["service", "check", "--manifest", "/tmp/candidate", "extra"],
    ] {
        let result = run(&args, &[]);
        assert_eq!(result.status.code(), Some(2), "args={args:?}");
        let value = body(&result);
        assert_eq!(value["error"]["code"], "service_usage");
        assert_eq!(value["service_started"], false);
        assert_eq!(value["journal_opened"], false);
        assert_eq!(value["live_access_checked"], false);
    }
}

#[test]
fn gateway_exec_never_logs_denial_or_falls_back_to_core_startup() {
    for (args, status) in [
        (vec!["service", "gateway-exec"], 2),
        (
            vec![
                "service",
                "gateway-exec",
                "--manifest",
                "/tmp/missing",
                "--plan",
                "/tmp/missing",
                "extra",
            ],
            2,
        ),
        (
            vec![
                "service",
                "gateway-exec",
                "--manifest",
                "/tmp/missing",
                "--plan",
                "/tmp/missing",
            ],
            1,
        ),
    ] {
        let result = run(&args, &[]);
        assert_eq!(result.status.code(), Some(status));
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }
    let result = run(
        &[
            "service",
            "gateway-exec",
            "--manifest",
            "/tmp/missing",
            "--plan",
            "/tmp/missing",
        ],
        &[("SECRETARY_PRIVATE_CANARY", "private")],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(output_text(&result).is_empty());
}

#[test]
fn conflicting_environment_is_denied_by_presence_even_when_empty_and_never_echoed() {
    for (key, env_value) in [
        ("SECRETARY_PRIVATE_CANARY", "secretary-env-canary"),
        ("secretary_lower_private_canary", "lowercase-env-canary"),
        ("GOOGLE_APPLICATION_CREDENTIALS", "google-path-canary"),
        ("HTTPS_PROXY", "proxy-canary"),
        ("HTTP_PROXY", ""),
        ("ALL_PROXY", ""),
        ("NO_PROXY", ""),
    ] {
        let result = check("/tmp/candidate.json", &[(key, env_value)]);
        assert_eq!(result.status.code(), Some(1), "key={key}");
        let response = body(&result);
        assert_eq!(response["error"]["code"], "service_environment_conflict");
        assert_eq!(response["service_started"], false);
        assert_eq!(response["journal_opened"], false);
        if !env_value.is_empty() {
            assert!(!output_text(&result).contains(env_value));
        }
        assert!(!output_text(&result).contains(key));
    }
}

#[test]
fn malformed_and_non_ascii_manifest_paths_are_rejected_without_echo() {
    for path in ["relative.json", "/tmp/é-candidate.json"] {
        let result = check(path, &[]);
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(body(&result)["error"]["code"], "manifest_path");
        assert!(!output_text(&result).contains(path));
        assert_eq!(body(&result)["service_started"], false);
        assert_eq!(body(&result)["journal_opened"], false);
    }
}

#[test]
fn canonical_manifest_path_cannot_pass_local_authority_check() {
    let path = format!("/tmp/secretary-service-check-{}.json", uuid::Uuid::new_v4());
    let result = check(&path, &[]);
    assert_eq!(result.status.code(), Some(1));
    let response = body(&result);
    let code = response["error"]["code"]
        .as_str()
        .expect("sanitized authority error");
    #[cfg(target_os = "linux")]
    assert_eq!(code, "unsafe_ancestor");
    #[cfg(not(target_os = "linux"))]
    assert_eq!(code, "file_authority_unsupported");
    assert!(!output_text(&result).contains(&path));
    assert_eq!(body(&result)["service_started"], false);
    assert_eq!(body(&result)["journal_opened"], false);
    assert!(!std::path::Path::new(&path).exists());
}

#[test]
fn public_typed_configuration_validation_rejects_schema_mutations() {
    let manifest: Manifest = serde_json::from_str(EXAMPLE).expect("canonical typed manifest");
    manifest.validate().expect("canonical manifest shape");

    let mut manifest_value: Value = serde_json::from_str(EXAMPLE).unwrap();
    for (field, invalid) in [
        ("schema", json!("ciel.readonly.manifest.v99")),
        ("profile", json!("controlled_write")),
        ("deployment_generation", json!("../bad")),
        ("owner_file", json!("/tmp/../owner")),
        ("budget_policy_file", json!("/tmp/é-policy.json")),
    ] {
        let mut changed = manifest_value.clone();
        changed[field] = invalid;
        let parsed: Manifest = serde_json::from_value(changed).expect("known fields still decode");
        assert!(parsed.validate().is_err(), "field={field}");
    }
    manifest_value["unknown"] = json!(true);
    assert!(serde_json::from_value::<Manifest>(manifest_value).is_err());

    let policy = json!({
        "schema":"ciel.readonly.service-policy.v1",
        "revision":"policy-revision-1",
        "core":{"uid":1001,"gid":1001},
        "gateway":{"uid":1002,"gid":1002},
        "listener":"127.0.0.1:8787",
        "filesystem":"linux-root-owned-no-acl.v1",
        "launcher":"ciel.readonly.launcher.v1",
        "endpoints":"google-production.v1"
    });
    let parsed: ServicePolicy = serde_json::from_value(policy.clone()).expect("typed policy");
    parsed.validate().expect("canonical policy values");
    for (field, invalid) in [
        ("listener", json!("0.0.0.0:8787")),
        ("filesystem", json!("permissive")),
        ("launcher", json!("unknown")),
        ("endpoints", json!("custom")),
    ] {
        let mut changed = policy.clone();
        changed[field] = invalid;
        let parsed: ServicePolicy = serde_json::from_value(changed).expect("known fields decode");
        assert!(parsed.validate().is_err(), "field={field}");
    }
    let mut unknown_policy = policy;
    unknown_policy["extra"] = json!(1);
    assert!(serde_json::from_value::<ServicePolicy>(unknown_policy).is_err());
}
