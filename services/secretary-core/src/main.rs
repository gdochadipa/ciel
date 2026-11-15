use secretary_core::google::GoogleAccess;
use secretary_core::{router, AppState, GoogleClient, Registry};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|value| value == "service")
        && args
            .get(1)
            .is_some_and(|value| value == "check-release-readonly")
    {
        std::process::exit(secretary_core::infrastructure::service_runtime::check_release(&args));
    }
    if args.first().is_some_and(|value| value == "service")
        && args.get(1).is_some_and(|value| value == "review-readonly")
    {
        std::process::exit(secretary_core::infrastructure::service_runtime::review(
            &args,
        ));
    }
    if args.first().is_some_and(|value| value == "service")
        && args.get(1).is_some_and(|value| value == "qualify-readonly")
    {
        std::process::exit(secretary_core::infrastructure::service_runtime::qualify(
            &args,
        ));
    }
    if args.first().is_some_and(|value| value == "service")
        && args.get(1).is_some_and(|value| value == "start-readonly")
    {
        std::process::exit(secretary_core::infrastructure::service_runtime::start(
            &args,
        ));
    }
    if args.first().is_some_and(|value| value == "service")
        && args.get(1).is_some_and(|value| value == "gateway-exec")
    {
        // A launch denial must not write into an unvalidated inherited handle.
        std::process::exit(secretary_core::presentation::service_cli::gateway_exec(
            &args,
        ));
    }
    if !args.is_empty() {
        let (body, status) = if args[0] == "service" {
            secretary_core::presentation::service_cli::run(&args)
        } else {
            secretary_core::presentation::registry_cli::run(&args)
        };
        println!("{body}");
        std::process::exit(status);
    }
    serve()
}
#[tokio::main]
async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let registry: Registry = secretary_core::infrastructure::config::read_json(
        std::path::Path::new(&std::env::var("SECRETARY_CONFIG")?),
    )?;
    let enabled = std::env::var("SECRETARY_WRITES_ENABLED").as_deref() == Ok("true");
    let task_deployment = task_deployment(
        enabled,
        std::env::var("SECRETARY_WRITE_TASKS_ENABLED")
            .ok()
            .as_deref(),
        std::env::var("SECRETARY_DEPLOYMENT_ID").ok(),
    )?;
    let token = std::env::var("SECRETARY_CORE_TOKEN")?;
    let google = if std::env::var("SECRETARY_MOCK_MODE").as_deref() == Ok("true") {
        GoogleClient::static_token(
            &std::env::var("SECRETARY_GOOGLE_BASE_URL")?,
            &std::env::var("SECRETARY_GOOGLE_TOKEN")?,
        )?
        .with_disposable_docs_base(
            &std::env::var("SECRETARY_GOOGLE_DOCS_BASE_URL")
                .unwrap_or(std::env::var("SECRETARY_GOOGLE_BASE_URL")?),
        )?
    } else {
        // Never print service-account contents or propagate provider error bodies.
        let credentials: serde_json::Value = serde_json::from_slice(&std::fs::read(
            std::env::var("GOOGLE_APPLICATION_CREDENTIALS")?,
        )?)?;
        GoogleClient::service_account_with_capabilities(
            credentials["client_email"]
                .as_str()
                .ok_or("missing client_email")?
                .into(),
            credentials["private_key"]
                .as_str()
                .ok_or("missing private_key")?,
            if !registry.spreadsheets.iter().any(|s| s.readable) {
                GoogleAccess::None
            } else if enabled
                && registry.spreadsheets.iter().any(|s| {
                    s.readable
                        && (s.create_tabs
                            || s.views
                                .iter()
                                .any(|v| v.structured() || v.writable_range.is_some()))
                })
            {
                GoogleAccess::Write
            } else {
                GoogleAccess::Read
            },
            if !registry.documents.iter().any(|d| d.readable) {
                GoogleAccess::None
            } else if enabled && registry.documents.iter().any(|d| d.readable && d.writable) {
                GoogleAccess::Write
            } else {
                GoogleAccess::Read
            },
        )?
    };
    let owner = std::env::var("SECRETARY_OWNER")?;
    let mut state = AppState::new(registry, token.clone(), owner.clone(), google)?;
    if let Ok(directory) = std::env::var("SECRETARY_STATE_DIR") {
        let journal = secretary_core::journal::Journal::open(std::path::Path::new(&directory))
            .await
            .map_err(|_| "journal initialization failed")?;
        let writes = if let Some(deployment) = task_deployment {
            // Explicit profile selection uses the same journal/writer/bus. It
            // disables legacy mutation routes rather than leaving a bypass.
            let tasks = secretary_core::journal::write_tasks::JournalWriteTasks::open(
                journal,
                &owner,
                &deployment,
            )
            .await
            .map_err(|_| "task ledger initialization failed")?;
            secretary_core::writes::Writes::with_task_ledger(tasks, enabled)
        } else {
            secretary_core::writes::Writes::new(journal, enabled)
        };
        state = state.with_writes(
            writes,
            std::env::var("SECRETARY_APPROVAL_TOKEN").unwrap_or_default(),
        )?;
    } else if enabled {
        return Err("SECRETARY_STATE_DIR is required when writes enabled".into());
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8787").await?;
    axum::serve(listener, router(state)).await?;
    Ok(())
}

fn task_deployment(
    writes_enabled: bool,
    selected: Option<&str>,
    deployment: Option<String>,
) -> Result<Option<String>, &'static str> {
    match selected {
        None | Some("false") => Ok(None),
        Some("true") if writes_enabled => deployment
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 128
                    && value.as_bytes()[0].is_ascii_alphanumeric()
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            })
            .map(Some)
            .ok_or("SECRETARY_DEPLOYMENT_ID is required for task writes"),
        Some("true") => Err("task writes require SECRETARY_WRITES_ENABLED=true"),
        Some(_) => Err("SECRETARY_WRITE_TASKS_ENABLED must be true or false"),
    }
}

#[cfg(test)]
mod tests {
    use super::task_deployment;

    #[test]
    fn task_profile_requires_explicit_enablement_and_stable_deployment() {
        assert_eq!(task_deployment(true, None, None), Ok(None));
        assert_eq!(task_deployment(true, Some("false"), None), Ok(None));
        assert_eq!(
            task_deployment(false, Some("true"), Some("deployment-v1".into())),
            Err("task writes require SECRETARY_WRITES_ENABLED=true")
        );
        for deployment in [None, Some("".into()), Some("bad\nidentity".into())] {
            assert_eq!(
                task_deployment(true, Some("true"), deployment),
                Err("SECRETARY_DEPLOYMENT_ID is required for task writes")
            );
        }
        assert_eq!(
            task_deployment(true, Some("yes"), Some("deployment-v1".into())),
            Err("SECRETARY_WRITE_TASKS_ENABLED must be true or false")
        );
        assert_eq!(
            task_deployment(true, Some("true"), Some("deployment-v1".into())),
            Ok(Some("deployment-v1".into()))
        );
    }
}
