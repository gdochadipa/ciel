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
        writes: bool,
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
        Self::service_account_with_writes(email, pem, false)
    }
    pub fn service_account_with_writes(
        email: String,
        pem: &str,
        writes: bool,
    ) -> Result<Self, String> {
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
                writes,
                cache: tokio::sync::Mutex::new(None),
            }),
        })
    }
    async fn token(&self) -> Result<String, ApiError> {
        match self.auth.as_ref() {
            Auth::Static(t) => Ok(t.clone()),
            Auth::Service {
                email,
                key,
                cache,
                writes,
            } => {
                let mut cache = cache.lock().await;
                if let Some((token, expiry)) = cache.as_ref() {
                    if *expiry > Instant::now() {
                        return Ok(token.clone());
                    }
                }
                let now = chrono::Utc::now().timestamp();
                let claims = json!({"iss":email,"scope":if *writes {"https://www.googleapis.com/auth/spreadsheets"}else{"https://www.googleapis.com/auth/spreadsheets.readonly"},"aud":"https://oauth2.googleapis.com/token","iat":now,"exp":now+3600});
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
    /// Exactly one send. Any ambiguous transport/server outcome must be reconciled by row operation_id.
    pub async fn write_values(
        &self,
        s: &Spreadsheet,
        v: &crate::registry::View,
        p: &crate::journal::Preview,
        row: Option<usize>,
    ) -> WriteOutcome {
        let result =
            tokio::time::timeout(Duration::from_secs(10), self.write_inner(s, v, p, row)).await;
        match result {
            Ok(Ok(outcome)) => outcome,
            _ => WriteOutcome::Uncertain,
        }
    }
    async fn write_inner(
        &self,
        s: &Spreadsheet,
        v: &crate::registry::View,
        p: &crate::journal::Preview,
        row: Option<usize>,
    ) -> Result<WriteOutcome, ApiError> {
        let schema = v.write_schema.as_ref().ok_or_else(ApiError::invalid)?;
        let token = self.token().await?;
        let title = v.title.replace('\'', "''");
        let request = if p.action == "append" {
            let last = (b'A' + 1 + schema.fields.len() as u8) as char;
            let range = format!("'{title}'!A2:{last}1001");
            let mut url =
                reqwest::Url::parse(&format!("{}/spreadsheets/{}/values/", self.base, s.id))
                    .map_err(|_| ApiError::invalid())?;
            url.path_segments_mut()
                .map_err(|_| ApiError::invalid())?
                .pop_if_empty()
                .push(&format!("{range}:append"));
            let mut values = vec![Value::Null; schema.fields.len() + 2];
            values[0] = json!(p.record_id);
            values[1] = json!(p.operation_id);
            for (name, f) in &schema.fields {
                let i = (f.column.as_bytes()[0] - b'A') as usize;
                values[i] = raw(&p.after[name]);
            }
            self.client
                .post(url)
                .query(&[
                    ("valueInputOption", "RAW"),
                    ("insertDataOption", "INSERT_ROWS"),
                    ("includeValuesInResponse", "true"),
                ])
                .json(&json!({"majorDimension":"ROWS","values":[values]}))
        } else {
            let row = row.ok_or_else(ApiError::invalid)?;
            let mut data = vec![
                json!({"range":format!("'{title}'!B{row}"),"majorDimension":"ROWS","values":[[p.operation_id]]}),
            ];
            for (name, value) in p.fields.as_object().ok_or_else(ApiError::invalid)? {
                let f = schema.fields.get(name).ok_or_else(ApiError::invalid)?;
                data.push(json!({"range":format!("'{title}'!{}{row}",f.column),"majorDimension":"ROWS","values":[[raw(value)]]}));
            }
            self.client
                .post(format!(
                    "{}/spreadsheets/{}/values:batchUpdate",
                    self.base, s.id
                ))
                .json(&json!({"valueInputOption":"RAW","data":data}))
        };
        let response = match request.bearer_auth(token).send().await {
            Ok(r) => r,
            Err(_) => return Ok(WriteOutcome::Uncertain),
        };
        let status = response.status();
        if status.is_success() {
            return Ok(if bounded(response).await.is_ok() {
                WriteOutcome::Accepted
            } else {
                WriteOutcome::Uncertain
            });
        }
        if matches!(status.as_u16(), 400 | 401 | 403 | 404 | 422) {
            Ok(WriteOutcome::Rejected)
        } else {
            Ok(WriteOutcome::Uncertain)
        }
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WriteOutcome {
    Accepted,
    Rejected,
    Uncertain,
}
fn raw(value: &Value) -> Value {
    if value.is_null() {
        json!("")
    } else {
        value.clone()
    }
}
