//! Closed GET-only source adapter. Raw captures and errors are redacted.
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::Value;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_DELAY: u64 = 86400;
static READ_GATE: OnceLock<Arc<Semaphore>> = OnceLock::new();
static PACING: Mutex<Option<tokio::time::Instant>> = Mutex::const_new(None);
/// Acquired before claiming work: queueing never consumes a lease or a DB connection.
/// Globally serializing is conservative across even unknown account identities.
pub async fn source_read_permit() -> OwnedSemaphorePermit {
    let permit = READ_GATE
        .get_or_init(|| Arc::new(Semaphore::new(1)))
        .clone()
        .acquire_owned()
        .await
        .expect("static gate");
    let until = *PACING.lock().await;
    if let Some(until) = until {
        tokio::time::sleep_until(until).await;
    }
    permit
}

#[derive(Clone, PartialEq, Eq)]
pub struct Identity {
    pub account_id: i64,
    pub account_domain: Option<String>,
    pub user_id: Option<i64>,
    pub display_name: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    PeopleExcludingTrash,
    PeopleIncludingTrash,
    Users,
    Stages,
    CustomFields,
}
impl Probe {
    pub fn key(self) -> &'static str {
        match self {
            Self::PeopleExcludingTrash => "people_excluding_trash",
            Self::PeopleIncludingTrash => "people_including_trash",
            Self::Users => "users",
            Self::Stages => "stages",
            Self::CustomFields => "custom_fields",
        }
    }
    fn path(self) -> &'static str {
        match self {
            Self::PeopleExcludingTrash => "people?limit=1&fields=id&includeTrash=false",
            Self::PeopleIncludingTrash => "people?limit=1&fields=id&includeTrash=true",
            Self::Users => "users?limit=1",
            Self::Stages => "stages?limit=1",
            Self::CustomFields => "customFields?limit=1",
        }
    }
    fn collection(self) -> &'static str {
        match self {
            Self::PeopleExcludingTrash | Self::PeopleIncludingTrash => "people",
            Self::Users => "users",
            Self::Stages => "stages",
            Self::CustomFields => "customfields",
        }
    }
}
#[derive(Clone)]
pub struct ProbeResult {
    pub status: u16,
    pub body: Vec<u8>,
    pub reported_total: Option<String>,
    pub retrieved_count: i32,
    pub continuation: bool,
    pub source_version: Option<String>,
}
impl std::fmt::Debug for ProbeResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProbeResult(REDACTED)")
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct Capture {
    pub status: u16,
    pub body: Vec<u8>,
    pub truncated: bool,
    pub source_version: Option<String>,
}
impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Capture(REDACTED)")
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReaderError {
    InvalidCredential,
    AccessDenied,
    RateLimited(Option<u64>),
    ResponseTooLarge,
    MalformedResponse,
    Unavailable,
    IdentityMismatch,
    InvalidTiming,
    Captured {
        error: Box<ReaderError>,
        capture: Capture,
    },
}
impl ReaderError {
    pub fn with_capture(self, capture: Capture) -> Self {
        Self::Captured {
            error: Box::new(self),
            capture,
        }
    }
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidCredential => "invalid_credential",
            Self::AccessDenied => "access_denied",
            Self::RateLimited(_) => "rate_limited",
            Self::ResponseTooLarge => "response_too_large",
            Self::MalformedResponse => "malformed_response",
            Self::Unavailable => "source_unavailable",
            Self::IdentityMismatch => "source_account_mismatch",
            Self::InvalidTiming => "invalid_rate_timing",
            Self::Captured { error, .. } => error.code(),
        }
    }
    pub fn split(self) -> (Self, Option<Capture>) {
        match self {
            Self::Captured { error, capture } => (*error, Some(capture)),
            other => (other, None),
        }
    }
}
#[async_trait]
pub trait FubReader: Send + Sync {
    async fn identity(&self, api_key: &str) -> Result<(Identity, Vec<u8>), ReaderError>;
    async fn probe(&self, api_key: &str, probe: Probe) -> Result<ProbeResult, ReaderError>;
}
/// Both headers are deployment configuration, never API inputs. Debug is redacted.
#[derive(Clone, Default)]
pub struct FubSystemConfig {
    pub name: Option<String>,
    pub key: Option<String>,
}
impl std::fmt::Debug for FubSystemConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FubSystemConfig(REDACTED)")
    }
}
pub struct HttpFubReader {
    client: reqwest::Client,
    system: Option<(HeaderValue, HeaderValue)>,
    #[cfg(test)]
    test_base: Option<String>,
}
impl HttpFubReader {
    pub fn new(
        system_name: Option<String>,
        system_key: Option<String>,
    ) -> Result<Self, ReaderError> {
        let system = match (system_name, system_key) {
            (None, None) => None,
            (Some(name), Some(key)) if !name.trim().is_empty() && !key.trim().is_empty() => Some((
                HeaderValue::from_str(&name).map_err(|_| ReaderError::Unavailable)?,
                HeaderValue::from_str(&key).map_err(|_| ReaderError::Unavailable)?,
            )),
            _ => return Err(ReaderError::Unavailable),
        };
        Ok(Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(REQUEST_TIMEOUT)
                .build()
                .map_err(|_| ReaderError::Unavailable)?,
            system,
            #[cfg(test)]
            test_base: None,
        })
    }
    async fn get(&self, api_key: &str, path: &str) -> Result<Capture, ReaderError> {
        let Some((system, key)) = &self.system else {
            return Err(ReaderError::Unavailable);
        };
        let base = "https://api.followupboss.com/v1/";
        #[cfg(test)]
        let base = self.test_base.as_deref().unwrap_or(base);
        let mut response = self
            .client
            .get(format!("{base}{path}"))
            .basic_auth(api_key, Some(""))
            .header("X-System", system)
            .header("X-System-Key", key)
            .send()
            .await
            .map_err(|_| ReaderError::Unavailable)?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let delay = rate_delay(status, &headers);
        match delay {
            Ok(Some(seconds)) => {
                let until = tokio::time::Instant::now() + Duration::from_secs(seconds);
                let mut pacing = PACING.lock().await;
                *pacing = Some(pacing.map_or(until, |old| old.max(until)));
            }
            Err(_) => {
                // Unknown timing pauses the affected check. A conservative
                // cooldown allows an explicit retry without poisoning every
                // connection for the lifetime of the process.
                let until = tokio::time::Instant::now() + Duration::from_secs(60);
                let mut pacing = PACING.lock().await;
                *pacing = Some(pacing.map_or(until, |old| old.max(until)));
            }
            _ => {}
        }
        let mut capture = Capture {
            status,
            body: Vec::new(),
            truncated: false,
            source_version: None,
        };
        // A selected version header is bounded and cannot inject log or UI text.
        capture.source_version = headers
            .get("x-api-version")
            .and_then(|v| v.to_str().ok())
            .filter(|v| {
                v.len() <= 64
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
            .map(str::to_owned);
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    let available = MAX_RESPONSE_BYTES - capture.body.len();
                    capture
                        .body
                        .extend_from_slice(&chunk[..chunk.len().min(available)]);
                    if chunk.len() > available {
                        capture.truncated = true;
                        return Err(ReaderError::ResponseTooLarge.with_capture(capture));
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    capture.truncated = true;
                    return Err(ReaderError::Unavailable.with_capture(capture));
                }
            }
        }
        let error = match status {
            401 => Some(ReaderError::InvalidCredential),
            403 => Some(ReaderError::AccessDenied),
            429 => Some(match delay {
                Ok(delay) => ReaderError::RateLimited(delay),
                Err(e) => e,
            }),
            500..=599 => Some(ReaderError::Unavailable),
            200..=299 => delay.err(),
            _ => Some(ReaderError::MalformedResponse),
        };
        if let Some(error) = error {
            Err(error.with_capture(capture))
        } else {
            Ok(capture)
        }
    }
}
fn rate_delay(status: u16, headers: &HeaderMap) -> Result<Option<u64>, ReaderError> {
    let seconds = |name: &str| -> Result<Option<u64>, ReaderError> {
        headers
            .get(name)
            .map(|v| {
                v.to_str()
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .filter(|v| *v <= MAX_DELAY)
                    .ok_or(ReaderError::InvalidTiming)
            })
            .transpose()
    };
    let retry = seconds("retry-after")?;
    let remaining = headers
        .get("x-ratelimit-remaining")
        .map(|v| {
            v.to_str()
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or(ReaderError::InvalidTiming)
        })
        .transpose()?;
    let exhausted = remaining == Some(0);
    let window = if exhausted {
        Some(seconds("x-ratelimit-window")?.ok_or(ReaderError::InvalidTiming)?)
    } else {
        None
    };
    Ok(match (retry, window) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b).or(if status == 429 { Some(60) } else { None }),
    })
}
pub fn parse_identity(body: &[u8]) -> Result<Identity, ReaderError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| ReaderError::MalformedResponse)?;
    let account_id = value
        .pointer("/account/id")
        .and_then(Value::as_i64)
        .filter(|v| *v > 0)
        .ok_or(ReaderError::MalformedResponse)?;
    let user_id = value
        .pointer("/user/id")
        .and_then(Value::as_i64)
        .filter(|v| *v > 0)
        .ok_or(ReaderError::MalformedResponse)?;
    Ok(Identity {
        account_id,
        user_id: Some(user_id),
        account_domain: value
            .pointer("/account/domain")
            .and_then(Value::as_str)
            .map(str::to_owned),
        display_name: value
            .pointer("/user/name")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}
#[async_trait]
impl FubReader for HttpFubReader {
    async fn identity(&self, api_key: &str) -> Result<(Identity, Vec<u8>), ReaderError> {
        let capture = self.get(api_key, "identity").await?;
        match parse_identity(&capture.body) {
            Ok(identity) => Ok((identity, capture.body)),
            Err(e) => Err(e.with_capture(capture)),
        }
    }
    async fn probe(&self, api_key: &str, probe: Probe) -> Result<ProbeResult, ReaderError> {
        let capture = self.get(api_key, probe.path()).await?;
        parse_probe(probe, capture)
    }
}
fn nonnegative_decimal(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) if n.as_u64().is_some() => Some(n.to_string()),
        Value::String(s)
            if !s.is_empty() && s.len() <= 30 && s.bytes().all(|b| b.is_ascii_digit()) =>
        {
            Some(
                match s.trim_start_matches('0') {
                    "" => "0",
                    v => v,
                }
                .to_string(),
            )
        }
        _ => None,
    }
}
fn parse_probe(probe: Probe, capture: Capture) -> Result<ProbeResult, ReaderError> {
    let parse = || -> Result<(Option<String>, i32, bool), ReaderError> {
        let value: Value =
            serde_json::from_slice(&capture.body).map_err(|_| ReaderError::MalformedResponse)?;
        let items = value
            .get(probe.collection())
            .and_then(Value::as_array)
            .ok_or(ReaderError::MalformedResponse)?;
        let count = i32::try_from(items.len()).map_err(|_| ReaderError::MalformedResponse)?;
        let meta = value.get("_metadata");
        let continuation = meta
            .and_then(|m| m.get("next"))
            .is_some_and(|v| !v.is_null())
            || meta
                .and_then(|m| m.get("nextLink"))
                .is_some_and(|v| !v.is_null());
        let mut total = meta
            .and_then(|m| m.get("total"))
            .and_then(nonnegative_decimal);
        let valid_collection = meta
            .and_then(|m| m.get("collection"))
            .and_then(Value::as_str)
            == Some(probe.collection());
        let valid_offset = meta
            .and_then(|m| m.get("offset"))
            .is_none_or(|v| nonnegative_decimal(v).as_deref() == Some("0"));
        if !valid_collection
            || !valid_offset
            || total.as_ref().is_some_and(|v| {
                v.parse::<u128>().unwrap_or(0) < count as u128
                    || (continuation && v.parse::<u128>().unwrap_or(0) <= count as u128)
            })
        {
            total = None;
        }
        Ok((total, count, continuation))
    };
    match parse() {
        Ok((reported_total, retrieved_count, continuation)) => Ok(ProbeResult {
            status: capture.status,
            body: capture.body,
            reported_total,
            retrieved_count,
            continuation,
            source_version: capture.source_version,
        }),
        Err(e) => Err(e.with_capture(capture)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    fn capture(body: Vec<u8>) -> Capture {
        Capture {
            status: 200,
            body,
            truncated: false,
            source_version: None,
        }
    }
    #[test]
    fn totals_are_bounded_normalized_and_honest() {
        for (total, expected) in [
            ("4", Some("4")),
            ("\"0004\"", Some("4")),
            ("-1", None),
            ("1.5", None),
            ("\"1234567890123456789012345678901\"", None),
            ("null", None),
            ("0", None),
        ] {
            let body = format!(
                r#"{{"_metadata":{{"collection":"customfields","total":{total}}},"customfields":[{{"id":1}}]}}"#
            );
            let r = parse_probe(Probe::CustomFields, capture(body.into_bytes())).unwrap();
            assert_eq!(r.reported_total.as_deref(), expected);
            assert_eq!(r.retrieved_count, 1);
        }
        let r = parse_probe(Probe::Users, capture(br#"{"users":[{}]}"#.to_vec())).unwrap();
        assert_eq!(r.reported_total, None);
        assert_eq!(r.retrieved_count, 1);
        let r = parse_probe(
            Probe::Users,
            capture(br#"{"_metadata":{"collection":"users","total":0},"users":[]}"#.to_vec()),
        )
        .unwrap();
        assert_eq!(r.reported_total.as_deref(), Some("0"));
    }
    #[test]
    fn identity_requires_positive_account_and_user() {
        for body in [
            r#"{"account":{"id":0},"user":{"id":1}}"#,
            r#"{"account":{"id":1},"user":{"id":-1}}"#,
            r#"{"account":{"id":1}}"#,
        ] {
            assert!(parse_identity(body.as_bytes()).is_err())
        }
    }
    #[test]
    fn source_timing_bounds_and_successful_budget_headers() {
        let mut h = HeaderMap::new();
        h.insert(
            "retry-after",
            HeaderValue::from_static("18446744073709551615"),
        );
        assert_eq!(rate_delay(429, &h), Err(ReaderError::InvalidTiming));
        h.insert("retry-after", HeaderValue::from_static("13"));
        h.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));
        h.insert("x-ratelimit-window", HeaderValue::from_static("20"));
        assert_eq!(rate_delay(200, &h), Ok(Some(20)));
        assert_eq!(rate_delay(429, &HeaderMap::new()), Ok(Some(60)));
    }
    async fn http_fixture(
        status: &str,
        headers: &str,
        body: Vec<u8>,
    ) -> (HttpFubReader, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n",
            body.len()
        );
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0; 16384];
            let n = socket.read(&mut buffer).await.unwrap();
            let request = String::from_utf8(buffer[..n].to_vec()).unwrap();
            socket.write_all(head.as_bytes()).await.unwrap();
            let _ = socket.write_all(&body).await;
            request
        });
        let mut reader = HttpFubReader::new(
            Some("synthetic-system".into()),
            Some("synthetic-system-secret".into()),
        )
        .unwrap();
        reader.test_base = Some(format!("http://{address}/v1/"));
        (reader, task)
    }
    #[tokio::test]
    async fn http_preserves_malformed_success_and_redacts_debug() {
        let raw = b"<html>synthetic-private-content</html>".to_vec();
        let (reader, task) = http_fixture("200 OK", "", raw.clone()).await;
        let error = reader
            .probe("synthetic-key", Probe::Stages)
            .await
            .unwrap_err();
        assert!(!format!("{error:?}").contains("private-content"));
        let (kind, evidence) = error.split();
        assert_eq!(kind, ReaderError::MalformedResponse);
        assert_eq!(evidence.unwrap().body, raw);
        let request = task.await.unwrap();
        assert!(request.starts_with("GET /v1/stages?limit=1 HTTP/1.1"));
        assert!(request
            .to_lowercase()
            .contains("x-system: synthetic-system"));
        assert!(request
            .to_lowercase()
            .contains("x-system-key: synthetic-system-secret"));
        assert!(request.to_lowercase().contains("authorization: basic "));
    }
    #[tokio::test]
    async fn http_does_not_follow_redirects_or_hostile_continuations() {
        let (reader, task) = http_fixture(
            "302 Found",
            "Location: http://127.0.0.1:1/steal\r\n",
            vec![],
        )
        .await;
        assert_eq!(
            reader.identity("key").await.err().unwrap().code(),
            "malformed_response"
        );
        task.await.unwrap();
        let(reader,task)=http_fixture("200 OK","",br#"{"_metadata":{"collection":"people","total":3,"nextLink":"http://127.0.0.1:1/steal"},"people":[{"id":1}]}"#.to_vec()).await;
        let r = reader
            .probe("key", Probe::PeopleIncludingTrash)
            .await
            .unwrap();
        assert!(r.continuation);
        assert!(task
            .await
            .unwrap()
            .starts_with("GET /v1/people?limit=1&fields=id&includeTrash=true HTTP/1.1"));
    }
    #[tokio::test]
    async fn http_bounds_and_preserves_oversized_prefix_and_disabled_reader_fails_closed() {
        let (reader, task) = http_fixture("200 OK", "", vec![b'x'; MAX_RESPONSE_BYTES + 100]).await;
        let (kind, evidence) = reader.probe("key", Probe::Users).await.unwrap_err().split();
        assert_eq!(kind, ReaderError::ResponseTooLarge);
        let e = evidence.unwrap();
        assert_eq!(e.body.len(), MAX_RESPONSE_BYTES);
        assert!(e.truncated);
        task.await.unwrap();
        assert_eq!(
            HttpFubReader::new(None, None)
                .unwrap()
                .identity("key")
                .await
                .err(),
            Some(ReaderError::Unavailable)
        );
        assert!(HttpFubReader::new(Some("name".into()), None).is_err());
    }
}
