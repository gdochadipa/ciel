use secretary_core::{router, AppState, GoogleClient, Registry};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry: Registry =
        serde_json::from_slice(&std::fs::read(std::env::var("SECRETARY_CONFIG")?)?)?;
    let google = if std::env::var("SECRETARY_MOCK_MODE").as_deref() == Ok("true") {
        GoogleClient::static_token(
            &std::env::var("SECRETARY_GOOGLE_BASE_URL")?,
            &std::env::var("SECRETARY_GOOGLE_TOKEN")?,
        )?
    } else {
        // Never print service-account contents or propagate provider error bodies.
        let credentials: serde_json::Value = serde_json::from_slice(&std::fs::read(
            std::env::var("GOOGLE_APPLICATION_CREDENTIALS")?,
        )?)?;
        GoogleClient::service_account(
            credentials["client_email"]
                .as_str()
                .ok_or("missing client_email")?
                .into(),
            credentials["private_key"]
                .as_str()
                .ok_or("missing private_key")?,
        )?
    };
    let state = AppState::new(
        registry,
        std::env::var("SECRETARY_CORE_TOKEN")?,
        std::env::var("SECRETARY_OWNER")?,
        google,
    )?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8787").await?;
    axum::serve(listener, router(state)).await?;
    Ok(())
}
