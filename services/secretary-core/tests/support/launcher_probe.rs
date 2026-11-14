//! Disposable acceptance probe, not an installed service or production CLI.
use secretary_core::{
    domain::service_config::ServicePolicy,
    infrastructure::{
        config::read_json, service_check::ReadOnlySnapshot,
        service_launcher::check_current_process, service_runtime::ReadOnlyRuntimeSnapshot,
    },
};
use std::{
    io::{Read, Write},
    path::Path,
    sync::mpsc,
    thread,
};

fn snapshot(path: &Path, mode: &str) -> Result<(), &'static str> {
    let snapshot = ReadOnlySnapshot::load(path)?;
    assert_eq!(snapshot.registry()?.spreadsheets.len(), 1);
    assert_eq!(snapshot.registry()?.documents.len(), 1);
    assert!(!snapshot.deployment_generation()?.is_empty());
    let report = snapshot.report()?;
    assert_eq!(report["readiness"], "incomplete");
    assert_eq!(report["blockers"].as_array().unwrap().len(), 4);
    if mode == "snapshot-no-cloexec" {
        // Deliberately corrupt one held regular handle's exec flags without
        // changing its inode or content. This is test-only, never a loader option.
        let descriptor = std::fs::read_dir("/proc/thread-self/fd")
            .map_err(|_| "probe_io")?
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
            .find(|number| {
                let mut info = std::mem::MaybeUninit::<libc::stat>::uninit();
                unsafe {
                    libc::fstat(*number, info.as_mut_ptr()) == 0
                        && info.assume_init().st_mode & libc::S_IFMT == libc::S_IFREG
                }
            })
            .ok_or("probe_io")?;
        assert_eq!(unsafe { libc::fcntl(descriptor, libc::F_SETFD, 0) }, 0);
    }
    if mode.ends_with("-wait") {
        println!("snapshot_loaded");
        std::io::stdout().flush().map_err(|_| "probe_io")?;
        std::io::stdin()
            .read_exact(&mut [0])
            .map_err(|_| "probe_io")?;
    }
    match mode {
        "snapshot-registry-wait" => {
            snapshot.registry()?;
        }
        "snapshot-generation-wait" => {
            snapshot.deployment_generation()?;
        }
        "snapshot-report-wait" => {
            snapshot.report()?;
        }
        _ => snapshot.check_current_process()?,
    }
    Ok(())
}

fn main() {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let mode = arguments.get(1).map(String::as_str);
    if mode == Some("runtime-server") {
        let result = runtime_server(
            Path::new(&arguments[0]),
            Path::new(&arguments[2]),
            &arguments[3],
        );
        if let Err(code) = result {
            println!("{code}");
            std::process::exit(1);
        }
        return;
    }
    if let Some(mode) = mode.filter(|mode| mode.starts_with("runtime")) {
        let result = runtime(Path::new(&arguments[0]), mode, Path::new(&arguments[2]));
        if mode == "runtime-quiet" {
            std::process::exit(if result == Err("launcher_inherited_descriptor") {
                42
            } else {
                43
            });
        }
        println!("{}", result.err().unwrap_or("passed"));
        std::process::exit(i32::from(result.is_err()));
    }
    if mode == Some("snapshot-gateway") {
        let result = ReadOnlySnapshot::load(Path::new(&arguments[0]))
            .and_then(|snapshot| snapshot.check_gateway_process(Path::new(&arguments[2])));
        println!("{}", result.err().unwrap_or("passed"));
        std::process::exit(i32::from(result.is_err()));
    }
    if let Some(mode) = mode.filter(|mode| mode.starts_with("snapshot")) {
        let result = snapshot(Path::new(&arguments[0]), mode);
        println!("{}", result.err().unwrap_or("passed"));
        std::process::exit(i32::from(result.is_err()));
    }
    let policy: ServicePolicy = read_json(Path::new(&arguments[0])).expect("synthetic policy");
    if mode == Some("descriptor-read-control") {
        use std::os::fd::FromRawFd;
        let descriptor: i32 = arguments[2].parse().unwrap();
        // A separate disposable child proves that credential drop did not revoke
        // this root-opened handle. The guard case itself must not read its body.
        let file = unsafe { std::fs::File::from_raw_fd(descriptor) };
        let mut bytes = Vec::new();
        file.take(128).read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"SYNTHETIC_INHERITED_PRIVATE_BODY");
        println!("control_read");
        return;
    }
    if mode == Some("descriptor-quiet") {
        // A real startup must not print errors into an unsafe inherited stdio
        // handle. Return a fixture sentinel only after checking the exact denial.
        std::process::exit(
            if check_current_process(&policy) == Err("launcher_inherited_descriptor") {
                42
            } else {
                43
            },
        );
    }
    if mode == Some("saved-root") {
        // Effective UID alone cannot detect this retained authority. There is
        // no exec after these disposable probe-only credential changes.
        assert_eq!(unsafe { libc::setgroups(0, std::ptr::null()) }, 0);
        assert_eq!(unsafe { libc::setresgid(1, 1, 0) }, 0);
        assert_eq!(unsafe { libc::setresuid(1, 1, 0) }, 0);
    }
    let (release, waiting) = mpsc::channel::<()>();
    let (ready, started) = mpsc::channel();
    let worker = (mode == Some("threaded")).then(|| {
        thread::spawn(move || {
            ready.send(()).unwrap();
            let _ = waiting.recv();
        })
    });
    if worker.is_some() {
        started.recv().unwrap();
    }
    let result = check_current_process(&policy);
    drop(release);
    if let Some(worker) = worker {
        worker.join().unwrap();
    }
    println!("{}", result.err().unwrap_or("passed"));
    std::process::exit(i32::from(result.is_err()));
}

fn runtime_server(path: &Path, plan: &Path, base: &str) -> Result<(), &'static str> {
    use secretary_core::{
        journal::{read_budget::JournalReadTasks, Journal},
        writes::Writes,
        AppState, GoogleClient,
    };
    use std::sync::Arc;
    let prepared = ReadOnlyRuntimeSnapshot::prepare(ReadOnlySnapshot::load(path)?, plan)?;
    let authority = Arc::new(prepared.into_runtime()?);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "probe_runtime")?;
    runtime.block_on(async {
        let input = authority.inputs()?;
        let registry = input.registry.clone();
        let owner = input.owner.to_owned();
        let core = input.core_token.to_owned();
        let operator = input.operator_token.to_owned();
        let generation = input.deployment_generation.to_owned();
        let policy = input.budget_policy.clone();
        let state = input.state_directory.to_path_buf();
        let journal = Journal::open_read_only_bound(&state, &owner, &generation)
            .await
            .map_err(|_| "probe_journal")?;
        let ledger = JournalReadTasks::open(journal.clone(), &owner, &generation, policy)
            .await
            .map_err(|_| "probe_budget")?;
        // Existing disposable adapter injection belongs only to this uninstalled
        // acceptance executable. Production startup has no provider override.
        let google = GoogleClient::static_token(&format!("{base}/v4"), "synthetic-provider")
            .and_then(|client| client.with_disposable_docs_base(&format!("{base}/v1")))
            .map_err(|_| "probe_client")?
            .read_only();
        let mut app = AppState::new_read_only(registry, core, owner, google)
            .map_err(|_| "probe_state")?
            .with_writes(Writes::new(journal, false), operator.clone())
            .map_err(|_| "probe_state")?
            .with_read_tasks(ledger, operator)
            .map_err(|_| "probe_state")?;
        app.bus = app
            .bus
            .with_runtime_authority(authority.clone())
            .map_err(|_| "probe_state")?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "probe_listener")?;
        let address = listener.local_addr().map_err(|_| "probe_listener")?;
        authority.check_runtime()?;
        println!("runtime_ready:{address}");
        std::io::stdout().flush().map_err(|_| "probe_io")?;
        axum::serve(listener, secretary_core::router(app))
            .with_graceful_shutdown(async {
                let _ = tokio::task::spawn_blocking(|| std::io::stdin().read_exact(&mut [0])).await;
            })
            .await
            .map_err(|_| "probe_server")
    })
}

fn barrier(message: &str) -> Result<(), &'static str> {
    println!("{message}");
    std::io::stdout().flush().map_err(|_| "probe_io")?;
    std::io::stdin()
        .read_exact(&mut [0])
        .map_err(|_| "probe_io")
}

fn runtime(path: &Path, mode: &str, plan: &Path) -> Result<(), &'static str> {
    let snapshot = ReadOnlySnapshot::load(path)?;
    if mode == "runtime-before-wait" {
        barrier("runtime_snapshot_loaded")?;
    }
    let runtime = ReadOnlyRuntimeSnapshot::prepare(snapshot, plan)?;
    if mode.starts_with("runtime-live") {
        let state = runtime.inputs()?.state_directory.to_path_buf();
        let authority = runtime.into_runtime()?;
        let (release, waiting) = mpsc::channel::<()>();
        let (ready, started) = mpsc::channel();
        let worker = thread::spawn(move || {
            ready.send(()).unwrap();
            waiting.recv().unwrap();
        });
        started.recv().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|_| "probe_io")?;
        let datagram = std::net::UdpSocket::bind("127.0.0.1:0").map_err(|_| "probe_io")?;
        let pair = std::os::unix::net::UnixStream::pair().map_err(|_| "probe_io")?;
        let external = if mode == "runtime-live-external-unix" {
            Some(
                std::os::unix::net::UnixStream::connect(std::env::args().nth(4).ok_or("probe_io")?)
                    .map_err(|_| "probe_io")?,
            )
        } else {
            None
        };
        use std::os::unix::fs::OpenOptionsExt;
        let name = if mode == "runtime-live-rogue" {
            "rogue"
        } else {
            "secretary.lock"
        };
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(state.join(name))
            .map_err(|_| "probe_io")?;
        let result = authority.check_runtime();
        drop(file);
        drop(listener);
        drop(datagram);
        drop(pair);
        drop(external);
        release.send(()).unwrap();
        worker.join().unwrap();
        if matches!(mode, "runtime-live-rogue" | "runtime-live-external-unix") {
            assert_eq!(result, Err("runtime_descriptor_denied"));
            assert_eq!(authority.check_runtime(), Err("runtime_fenced"));
        } else {
            result?;
            authority.inputs()?;
        }
        return Ok(());
    }
    {
        let inputs = runtime.inputs()?;
        assert_eq!(inputs.owner, " exact synthetic owner ");
        assert_eq!(inputs.core_token, "SYNTHETIC-CORE-0123456789-abcdefgh");
        assert_eq!(inputs.operator_token, "SYNTHETIC-OPERATOR-9876543210-zyxw");
        assert_eq!(
            inputs.google_principal,
            "synthetic@test-project.iam.gserviceaccount.com"
        );
        assert_eq!(inputs.registry.spreadsheets.len(), 1);
        assert_eq!(inputs.registry.documents.len(), 1);
    }
    if mode == "runtime-wait" {
        barrier("runtime_prepared")?;
    }
    runtime.inputs()?;
    Ok(())
}
