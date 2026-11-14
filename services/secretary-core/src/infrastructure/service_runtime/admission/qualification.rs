//! Bounded live Core qualification. The reserved loopback port identifies the
//! instance, while the HTTP router is driven privately with no accept loop.
use super::{binding, load_snapshot, protected, sha256};
use crate::{
    domain::service_admission::{
        digest, Case, EvidenceClass, Observation, Plan, Run, MAX_CAPTURE_TOTAL, MAX_RESPONSE,
        UNCHECKED,
    },
    infrastructure::service_runtime::{
        google_client, new_runtime, ReadOnlyRuntimeAuthority, ReadOnlyRuntimeSnapshot,
    },
    AppState, Registry,
};
use axum::{
    body::{to_bytes, Body},
    http::Request,
    Router,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    sync::Arc,
    time::Instant,
};
use tower::ServiceExt;

#[derive(Serialize)]
struct Capture {
    response: String,
}

#[derive(Serialize)]
struct Bundle {
    run: Run,
    captures: Vec<Capture>,
}

fn bearer_kind(case: &Case) -> &'static str {
    match case {
        Case::MissingBearer => "missing",
        Case::WrongBearer => "wrong",
        _ => "configured_core",
    }
}

pub(super) fn request_digest(case: &Case) -> String {
    let (tool, input, _) = case.request();
    let mut value = json!({"tool":tool,"input":input,"bearer":bearer_kind(case)});
    value.sort_all_objects();
    sha256(value.to_string().as_bytes())
}

fn check_output(output: &Path, state: &Path) -> Result<(), &'static str> {
    let parent = output
        .parent()
        .filter(|path| path.is_absolute())
        .ok_or("qualification_output_invalid")?;
    if fs::canonicalize(parent).as_deref().ok() != Some(parent)
        || output.starts_with(state)
        || output.file_name().and_then(|name| name.to_str()) != Some("qualification.json")
    {
        return Err("qualification_output_invalid");
    }
    let info = fs::symlink_metadata(parent).map_err(|_| "qualification_output_invalid")?;
    if !info.is_dir()
        || info.uid() != unsafe { libc::geteuid() }
        || info.mode() & 0o7777 != 0o700
        || fs::read_dir(parent)
            .map_err(|_| "qualification_output_invalid")?
            .next()
            .is_some()
    {
        return Err("qualification_output_invalid");
    }
    Ok(())
}

fn save(output: &Path, bundle: &Bundle) -> Result<String, &'static str> {
    // A temporary file can survive interruption. Reviewers accept only the
    // complete final name and an independently observed zero exit status.
    let temporary = output.with_file_name(".qualification.pending");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&temporary)
        .map_err(|_| "qualification_output_failed")?;
    let bytes = serde_json::to_vec(bundle).map_err(|_| "qualification_output_failed")?;
    file.write_all(&bytes)
        .map_err(|_| "qualification_output_failed")?;
    file.flush()
        .and_then(|()| file.sync_all())
        .map_err(|_| "qualification_output_failed")?;
    fs::hard_link(&temporary, output).map_err(|_| "qualification_output_failed")?;
    fs::remove_file(temporary).map_err(|_| "qualification_output_failed")?;
    fs::File::open(output.parent().ok_or("qualification_output_failed")?)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "qualification_output_failed")?;
    Ok(sha256(&bytes))
}

fn check_supervisor(expected: i32) -> Result<(), &'static str> {
    use crate::infrastructure::{gateway_launch::read_proc, service_launcher::status};
    if expected <= 1 || unsafe { libc::getppid() } != expected {
        return Err("qualification_supervisor_required");
    }
    let authority = status::parse(&read_proc(&format!("/proc/{expected}/status"), 65_536)?)?;
    if authority.uids != [0; 4]
        || unsafe {
            libc::prctl(
                libc::PR_SET_PDEATHSIG,
                libc::SIGKILL as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            )
        } != 0
        || unsafe { libc::getppid() } != expected
    {
        return Err("qualification_supervisor_required");
    }
    Ok(())
}

fn arm_process_deadline(seconds: u64) -> Result<(), &'static str> {
    // The child has its own kernel deadline even if the supervisor disappears
    // or the async runtime is stuck in a synchronous filesystem observation.
    let mut mask = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
    unsafe {
        if libc::signal(libc::SIGALRM, libc::SIG_DFL) == libc::SIG_ERR
            || libc::sigemptyset(mask.as_mut_ptr()) != 0
            || libc::sigaddset(mask.as_mut_ptr(), libc::SIGALRM) != 0
            || libc::pthread_sigmask(libc::SIG_UNBLOCK, mask.as_ptr(), std::ptr::null_mut()) != 0
        {
            return Err("qualification_deadline_failed");
        }
        libc::alarm(seconds as libc::c_uint);
    }
    Ok(())
}

fn cursor_matches(cursor: &Value, binding: String, count: usize) -> bool {
    cursor.is_null() || (count == 1 && cursor.as_str() == Some(&format!("{binding}:1")))
}

fn sheet_matches(registry: &Registry, alias: &str, view: &str, data: &Value) -> Option<bool> {
    use crate::domain::projection::read_binding;
    let sheet = registry.lookup(alias).ok()?;
    let selected = sheet
        .views
        .iter()
        .find(|candidate| candidate.name == view)?;
    let records = data.get("records")?.as_array()?;
    let source_revision = data.get("source_revision")?.as_str()?;
    let cursor = data.get("next_cursor")?;
    let binding = read_binding(&sheet.id, view, None, &registry.version, source_revision);
    Some(
        digest(source_revision)
            && data["mapping_version"] == registry.version
            && records.len() <= 1
            && records.iter().all(|record| {
                let source = &record["source"];
                source["spreadsheet_id"] == sheet.id
                    && source["sheet_id"] == selected.sheet_id
                    && source["source_revision"] == source_revision
                    && source["mapping_version"] == registry.version
            })
            && cursor_matches(cursor, binding, records.len()),
    )
}

fn doc_matches(registry: &Registry, alias: &str, tab: &str, data: &Value) -> Option<bool> {
    use crate::domain::documents::read_binding_for_source;
    let doc = registry.document(alias).ok()?;
    let text = data.get("text")?.as_str()?;
    let count = text.chars().count();
    let source = data.get("source")?;
    let fingerprint = source.get("source_fingerprint")?.as_str()?;
    let cursor = data.get("next_cursor")?;
    let binding = read_binding_for_source(&doc.id, tab, fingerprint, &registry.version);
    Some(
        digest(fingerprint)
            && count <= 1
            && source["document_id"] == doc.id
            && source["tab_id"] == tab
            && source["start_character"] == 0
            && source["end_character"] == count
            && source["mapping_version"] == registry.version
            && cursor_matches(cursor, binding, count),
    )
}

async fn observe(
    app: &Router,
    registry: &Registry,
    token: &str,
    case: &Case,
) -> Result<(Observation, Capture), &'static str> {
    let (tool, input, expected) = case.request();
    let mut request =
        Request::post(format!("/v1/tools/{tool}")).header("content-type", "application/json");
    request = match case {
        Case::MissingBearer => request,
        Case::WrongBearer => request.header("authorization", "Bearer invalid!"),
        _ => request.header("authorization", format!("Bearer {token}")),
    };
    let request = request
        .body(Body::from(input.to_string()))
        .map_err(|_| "qualification_request")?;
    let response = app
        .clone()
        .oneshot(request)
        .await
        .map_err(|_| "qualification_request")?;
    if response.status().as_u16() != expected {
        return Err("qualification_assertion");
    }
    let body = to_bytes(response.into_body(), MAX_RESPONSE)
        .await
        .map_err(|_| "qualification_capture_bound")?;
    let value: Value = serde_json::from_slice(&body).map_err(|_| "qualification_assertion")?;
    let valid = match case {
        Case::MissingBearer | Case::WrongBearer => value["error"]["code"] == "unauthorized",
        Case::ReadonlyMutation => value["error"]["code"] == "readonly_profile",
        Case::Sheet { spreadsheet, view } => {
            value["ok"] == true
                && sheet_matches(registry, spreadsheet, view, &value["data"]) == Some(true)
        }
        Case::Document { document, tab_id } => {
            value["ok"] == true
                && doc_matches(registry, document, tab_id, &value["data"]) == Some(true)
        }
    };
    if !valid {
        return Err("qualification_assertion");
    }
    let observation = Observation {
        case: case.clone(),
        status: expected,
        request_sha256: request_digest(case),
        response_sha256: sha256(&body),
        response_bytes: body.len(),
    };
    let response = std::str::from_utf8(&body)
        .map_err(|_| "qualification_assertion")?
        .to_owned();
    Ok((observation, Capture { response }))
}

async fn run_cases(
    authority: Arc<ReadOnlyRuntimeAuthority>,
    plan: &Plan,
) -> Result<(Vec<Observation>, Vec<Capture>), &'static str> {
    let input = authority.inputs()?;
    let mut state = AppState::new_read_only(
        input.registry.clone(),
        input.core_token.to_owned(),
        input.owner.to_owned(),
        google_client(&input)?,
    )
    .map_err(|_| "qualification_state")?;
    state.bus = state
        .bus
        .with_runtime_authority(authority.clone())
        .map_err(|_| "qualification_state")?;
    let app = crate::router(state);
    let mut total = 0;
    let mut observations = Vec::new();
    let mut captures = Vec::new();
    for case in &plan.cases {
        authority.check_runtime()?;
        let (observation, capture) = observe(&app, input.registry, input.core_token, case).await?;
        total += observation.response_bytes;
        if total > MAX_CAPTURE_TOTAL {
            return Err("qualification_capture_bound");
        }
        observations.push(observation);
        captures.push(capture);
    }
    authority.check_runtime()?;
    Ok((observations, captures))
}

pub(in crate::infrastructure::service_runtime) fn run(
    manifest: &Path,
    gateway_plan: &Path,
    plugin: &Path,
    plan_path: &Path,
    output: &Path,
    supervisor: i32,
) -> Result<(), &'static str> {
    check_supervisor(supervisor)?;
    let mut snapshot = load_snapshot(manifest, plugin)?;
    let (plan, plan_sha256): (Plan, _) = protected(&mut snapshot, plan_path)?;
    let generation = snapshot.deployment_generation()?.to_owned();
    plan.validate(&generation, snapshot.registry()?)?;
    let state = Path::new(&snapshot.checked.manifest.state_directory);
    check_output(output, state)?;
    arm_process_deadline(plan.wall_seconds)?;
    let started = Instant::now();
    let started_at = chrono::Utc::now().timestamp();
    let mut prepared = ReadOnlyRuntimeSnapshot::prepare(snapshot, gateway_plan)?;
    let binding = binding(&mut prepared, plugin)?;
    let authority = Arc::new(prepared.into_runtime()?);
    let reservation = std::net::TcpListener::bind("127.0.0.1:8787")
        .map_err(|_| "qualification_port_unavailable")?;
    let runtime = new_runtime()?;
    let (observations, captures) = runtime.block_on(run_cases(authority, &plan))?;
    drop(runtime);
    drop(reservation);
    let run = Run {
        schema: "ciel.readonly.run.v1".into(),
        evidence_class: EvidenceClass::OperatorLiveCore,
        run_id: uuid::Uuid::new_v4().to_string(),
        instance_pid: std::process::id(),
        deployment_generation: generation,
        binding,
        plan_sha256,
        started_at,
        finished_at: chrono::Utc::now().timestamp(),
        elapsed_ms: started
            .elapsed()
            .as_millis()
            .try_into()
            .map_err(|_| "qualification_deadline_failed")?,
        core_version: env!("CARGO_PKG_VERSION").into(),
        observations,
        not_checked: UNCHECKED.to_vec(),
    };
    let digest = save(output, &Bundle { run, captures })?;
    // Only a digest crosses the supervisor's inherited private stdout pipe.
    writeln!(std::io::stdout().lock(), "{digest}").map_err(|_| "qualification_output_failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{PortFuture, WorkspacePort, WriteOutcome};
    use crate::domain::{
        error::CoreError,
        operation::Preview,
        registry::{Document, Spreadsheet, View},
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FixtureSource(Arc<AtomicUsize>);

    impl WorkspacePort for FixtureSource {
        fn read_document<'a>(
            &'a self,
            _: &'a Document,
        ) -> PortFuture<'a, Result<Value, CoreError>> {
            Box::pin(async move {
                self.0.fetch_add(1, Ordering::Relaxed);
                Ok(json!({
                    "documentId": "demo-document", "title": "Fixture", "revisionId": "r1",
                    "tabs": [{"tabProperties": {"tabId": "t.0", "title": "Main"},
                        "documentTab": {"body": {"content": [{"paragraph": {"elements": [
                            {"textRun": {"content": "é🙂 fixture\n"}}
                        ]}}]}}}]
                }))
            })
        }

        fn read<'a>(&'a self, _: &'a Spreadsheet) -> PortFuture<'a, Result<Value, CoreError>> {
            Box::pin(async { panic!("Auth and mutation probes must not read Sheets") })
        }

        fn write_values<'a>(
            &'a self,
            _: &'a Spreadsheet,
            _: &'a View,
            _: &'a Preview,
            _: Option<usize>,
        ) -> PortFuture<'a, WriteOutcome> {
            Box::pin(async { panic!("Qualification must never write to the provider") })
        }
    }

    #[tokio::test]
    async fn probes_observe_auth_denials_and_bound_unicode_source_through_the_actual_router() {
        let registry: Registry = serde_json::from_str(include_str!(
            "../../../../../../config/workspace.example.json"
        ))
        .unwrap();
        let reads = Arc::new(AtomicUsize::new(0));
        let token = "SYNTHETIC-CORE-0123456789-abcdefgh";
        let state = AppState::new_read_only(
            registry.clone(),
            token.into(),
            "owner".into(),
            FixtureSource(reads.clone()),
        )
        .unwrap();
        let app = crate::router(state);
        for case in [
            Case::MissingBearer,
            Case::WrongBearer,
            Case::ReadonlyMutation,
        ] {
            observe(&app, &registry, token, &case).await.unwrap();
        }
        assert_eq!(reads.load(Ordering::Relaxed), 0);
        let case = Case::Document {
            document: "notes".into(),
            tab_id: "t.0".into(),
        };
        let (observation, capture) = observe(&app, &registry, token, &case).await.unwrap();
        assert_eq!(
            observation.response_sha256,
            sha256(capture.response.as_bytes())
        );
        let mut value: Value = serde_json::from_str(&capture.response).unwrap();
        assert_eq!(value["data"]["text"], "é");
        assert_eq!(reads.load(Ordering::Relaxed), 1);
        value["data"]["next_cursor"] = json!("another-source:1");
        assert_eq!(
            doc_matches(&registry, "notes", "t.0", &value["data"]),
            Some(false)
        );
    }
}
