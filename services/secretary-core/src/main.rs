use secretary_core::google::GoogleAccess;
use secretary_core::{router, AppState, GoogleClient, Registry};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry: Registry =
        serde_json::from_slice(&std::fs::read(std::env::var("SECRETARY_CONFIG")?)?)?;
    let enabled = std::env::var("SECRETARY_WRITES_ENABLED").as_deref() == Ok("true");
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
    let mut state = AppState::new(
        registry,
        token.clone(),
        std::env::var("SECRETARY_OWNER")?,
        google,
    )?;
    if let Ok(directory) = std::env::var("SECRETARY_STATE_DIR") {
        let journal = secretary_core::journal::Journal::open(std::path::Path::new(&directory))
            .await
            .map_err(|_| "journal initialization failed")?;
        let writes = secretary_core::writes::Writes::new(journal, enabled);
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
