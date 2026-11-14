//! Explicit operator commands; unknown input never falls back to Core startup.
use serde_json::{json, Value};
use std::{ffi::OsString, path::Path};

/// Execute only `service check --manifest FILE` and return sanitized JSON/status.
/// Exit 3 means incomplete, exit 1 denied, and exit 2 invalid usage.
///
/// # Examples
///
/// ```
/// use secretary_core::presentation::service_cli::run;
/// let args = [std::ffi::OsString::from("service")];
/// let (_, status) = run(&args);
/// assert_eq!(status, 2);
/// ```
pub fn run(args: &[OsString]) -> (Value, i32) {
    let base = |code: &str| {
        json!({"ok":false,"mode":"offline","readiness":"denied",
        "error":{"code":code},"configuration_changed":false,"activation_changed":false,
        "live_access_checked":false,"service_started":false,"journal_opened":false})
    };
    if args.len() != 4 || args[0] != "service" || args[1] != "check" || args[2] != "--manifest" {
        return (base("service_usage"), 2);
    }
    // Reject key presence, including empty values; never inspect or print values.
    if std::env::vars_os().any(|(key, _)| conflicts(&key.to_string_lossy())) {
        return (base("service_environment_conflict"), 1);
    }
    match crate::infrastructure::service_check::inspect(Path::new(&args[3])) {
        Ok(report) => (report, 3),
        Err(code) => (base(code), 1),
    }
}

fn conflicts(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    upper.starts_with("SECRETARY_")
        || upper == "GOOGLE_APPLICATION_CREDENTIALS"
        || matches!(
            upper.as_str(),
            "HTTP_PROXY" | "HTTPS_PROXY" | "ALL_PROXY" | "NO_PROXY"
        )
}

/// Replace a root launcher with the explicitly selected unprivileged Gateway.
///
/// Accepts only `service gateway-exec --manifest FILE --plan FILE`. Returns status
/// 2 for usage or 1 for a denied/failed launch, without touching inherited stdio.
/// On success exec replaces this process; it never returns or starts Core.
///
/// # Examples
/// ```
/// use secretary_core::presentation::service_cli::gateway_exec;
/// assert_eq!(gateway_exec(&[]), 2);
/// ```
pub fn gateway_exec(args: &[OsString]) -> i32 {
    if args.len() != 6
        || args[0] != "service"
        || args[1] != "gateway-exec"
        || args[2] != "--manifest"
        || args[4] != "--plan"
    {
        return 2;
    }
    if std::env::vars_os().any(|(key, _)| conflicts(&key.to_string_lossy())) {
        return 1;
    }
    #[cfg(target_os = "linux")]
    {
        let _ = crate::infrastructure::gateway_launch::exec_gateway(
            Path::new(&args[3]),
            Path::new(&args[5]),
        );
    }
    1
}
