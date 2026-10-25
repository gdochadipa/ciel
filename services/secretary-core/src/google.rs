use crate::{registry::Spreadsheet, ApiError};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct GoogleClient {
    client: reqwest::Client,
    base: Arc<str>,
    auth: Arc<Auth>,
}
enum Auth {
    Static(String),
    Service {
        email: String,
        key: jsonwebtoken::EncodingKey,
        cache: tokio::sync::Mutex<Option<(String, Instant)>>,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceAccount {
    pub client_email: String,
    pub private_key: String,
}
impl GoogleClient {
    fn client() -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(3))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "HTTP client initialization failed".into())
    }
    /// Test/disposable provider injection. Production main always constructs service_account.
    pub fn static_token(base: &str, token: &str) -> Result<Self, String> {
        let url = reqwest::Url::parse(base).map_err(|_| "invalid provider URL")?;
        if url.scheme() != "https"
            && !(url.scheme() == "http"
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))
        {
            return Err("provider URL must use TLS or loopback".into());
        }
        Ok(Self {
            client: Self::client()?,
            base: base.trim_end_matches('/').into(),
            auth: Arc::new(Auth::Static(token.into())),
        })
    }
    pub fn service_account(email: String, pem: &str) -> Result<Self, String> {
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes())
            .map_err(|_| "invalid service account RSA key")?;
        if email.is_empty() {
            return Err("missing service account email".into());
        }
        Ok(Self {
            client: Self::client()?,
            base: "https://sheets.googleapis.com/v4".into(),
            auth: Arc::new(Auth::Service {
                email,
                key,
                cache: tokio::sync::Mutex::new(None),
            }),
        })
    }
    async fn token(&self) -> Result<String, ApiError> {
        match self.auth.as_ref() {
            Auth::Static(t) => Ok(t.clone()),
            Auth::Service { email, key, cache } => {
                let mut cache = cache.lock().await;
                if let Some((token, expiry)) = cache.as_ref() {
                    if *expiry > Instant::now() {
                        return Ok(token.clone());
                    }
                }
                let now = chrono::Utc::now().timestamp();
                let claims = json!({"iss":email,"scope":"https://www.googleapis.com/auth/spreadsheets.readonly","aud":"https://oauth2.googleapis.com/token","iat":now,"exp":now+3600});
                let assertion = jsonwebtoken::encode(
                    &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
                    &claims,
                    key,
                )
                .map_err(|_| ApiError::upstream())?;
                let response = self
                    .client
                    .post("https://oauth2.googleapis.com/token")
                    .form(&[
                        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                        ("assertion", &assertion),
                    ])
                    .send()
                    .await
                    .map_err(|_| ApiError::upstream())?;
                if !response.status().is_success() {
                    return Err(ApiError::upstream());
                }
                let body = bounded(response).await?;
                let token = body["access_token"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(ApiError::upstream)?
                    .to_owned();
                let ttl = body["expires_in"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(3600)
                    .saturating_sub(60);
                *cache = Some((token.clone(), Instant::now() + Duration::from_secs(ttl)));
                Ok(token)
            }
        }
    }
    pub async fn read(&self, s: &Spreadsheet) -> Result<Value, ApiError> {
        // Bound whole operation, including OAuth and retries, independently of socket timeout.
        tokio::time::timeout(Duration::from_secs(10), self.read_inner(s))
            .await
            .map_err(|_| ApiError::upstream())?
    }
    async fn read_inner(&self, s: &Spreadsheet) -> Result<Value, ApiError> {
        let token = self.token().await?;
        let mut params=vec![("includeGridData".to_owned(),"true".to_owned()),("fields".to_owned(),"spreadsheetId,sheets(properties(sheetId,title),merges,data(startRow,startColumn,rowData(values(userEnteredValue,effectiveValue,formattedValue))))".to_owned())];
        for v in &s.views {
            params.push((
                "ranges".into(),
                format!("'{}'!{}", v.title.replace('\'', "''"), v.range),
            ))
        }
        for attempt in 0..=2 {
            let response = self
                .client
                .get(format!("{}/spreadsheets/{}", self.base, s.id))
                .bearer_auth(&token)
                .query(&params)
                .send()
                .await;
            match response {
                Ok(r) if r.status().is_success() => return bounded(r).await,
                Ok(r) if r.status().as_u16() == 429 || r.status().is_server_error() => {}
                Err(e) if e.is_timeout() || e.is_connect() => {}
                _ => return Err(ApiError::upstream()),
            }
            if attempt < 2 {
                tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
            }
        }
        Err(ApiError::upstream())
    }
}
async fn bounded(response: reqwest::Response) -> Result<Value, ApiError> {
    const MAX: usize = 2 * 1024 * 1024;
    if response.content_length().is_some_and(|n| n > MAX as u64) {
        return Err(ApiError::upstream());
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ApiError::upstream())?;
        if bytes.len() + chunk.len() > MAX {
            return Err(ApiError::upstream());
        }
        bytes.extend_from_slice(&chunk)
    }
    serde_json::from_slice(&bytes).map_err(|_| ApiError::upstream())
}
