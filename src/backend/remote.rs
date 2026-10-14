use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::header::{ETAG, IF_NONE_MATCH};
use reqwest::redirect::Policy;
use reqwest::{Client, StatusCode, Url};
use toml::Table;
use toml_schema::Schema;

const MAX_BODY_BYTES: usize = 1024 * 1024;
const MAX_REDIRECTS: usize = 3;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(10);
const CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const ERROR_TTL: Duration = Duration::from_secs(30);
const MAX_CACHE_ENTRIES: usize = 64;

/// Validate the parts of a URL that can be checked without network access.
/// Plain http is only accepted for tests, which talk to a local server.
pub fn validate_url(url: &Url, allow_http: bool) -> Result<(), String> {
    if url.scheme() != "https" && !(allow_http && url.scheme() == "http") {
        return Err(format!(
            "only https schema URLs are supported, not '{}'",
            url.scheme()
        ));
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err("schema URLs with credentials are not supported".to_string());
    }

    if url.host_str().is_none() {
        return Err("schema URL has no host".to_string());
    }
    Ok(())
}

enum Fetched {
    Schema {
        schema: Schema,
        etag: Option<String>,
    },
    NotModified,
}

pub struct Fetcher {
    client: Client,
    allow_http: bool,
}

impl Fetcher {
    fn new(allow_http: bool) -> Self {
        let client = Client::builder()
            .https_only(!allow_http)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TOTAL_TIMEOUT)
            .user_agent(concat!("toml-schema-lsp/", env!("CARGO_PKG_VERSION")))
            .no_proxy()
            .redirect(Policy::custom(move |attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    return attempt.error("too many redirects");
                }

                // Only the host which was allowed may be contacted, not where it redirects to
                if attempt.previous().first().and_then(Url::host_str) != attempt.url().host_str() {
                    return attempt.error("redirect to another host");
                }

                match validate_url(attempt.url(), allow_http) {
                    Ok(()) => attempt.follow(),
                    Err(e) => attempt.error(e),
                }
            }))
            .build()
            .expect("static client configuration is valid");

        Self { client, allow_http }
    }

    async fn fetch(&self, url: &Url, etag: Option<&str>) -> Result<Fetched, String> {
        validate_url(url, self.allow_http)?;

        let mut request = self.client.get(url.clone());

        if let Some(etag) = etag {
            request = request.header(IF_NONE_MATCH, etag);
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("request failed: {}", short_error(&e)))?;

        match response.status() {
            StatusCode::NOT_MODIFIED if etag.is_some() => return Ok(Fetched::NotModified),
            StatusCode::OK => {}
            status => return Err(format!("server responded with {status}")),
        }

        if response
            .content_length()
            .is_some_and(|len| len > MAX_BODY_BYTES as u64)
        {
            return Err(format!("schema exceeds {MAX_BODY_BYTES} bytes"));
        }

        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        // Content-Length can be absent or wrong, so enforce the limit while streaming
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("download failed: {}", short_error(&e)))?;
            if body.len() + chunk.len() > MAX_BODY_BYTES {
                return Err(format!("schema exceeds {MAX_BODY_BYTES} bytes"));
            }
            body.extend_from_slice(&chunk);
        }

        let text = String::from_utf8(body).map_err(|_| "schema is not valid UTF-8".to_string())?;
        let table = toml::from_str::<Table>(&text)
            .map_err(|e| format!("schema is not valid TOML: {}", e.message()))?;
        let schema = Schema::from_table(url.as_str().into(), table)?;

        Ok(Fetched::Schema { schema, etag })
    }
}

// Only the failure class is shown, never details from the server
fn short_error(e: &reqwest::Error) -> &'static str {
    if e.is_timeout() {
        "timed out"
    } else if e.is_redirect() {
        "blocked or too many redirects"
    } else if e.is_connect() {
        "could not connect"
    } else {
        "network error"
    }
}

struct Entry {
    schema: Option<Arc<Schema>>,
    etag: Option<String>,
    fetched_at: Instant,
    error: Option<(String, Instant)>,
}

/// Cache of remote schemas, each URL has its own lock so concurrent validations share one request
pub struct RemoteSchemas {
    fetcher: Fetcher,
    entries: Mutex<HashMap<Url, Arc<tokio::sync::Mutex<Option<Entry>>>>>,
}

impl Default for RemoteSchemas {
    fn default() -> Self {
        Self::new(Fetcher::new(false))
    }
}

impl RemoteSchemas {
    pub fn new(fetcher: Fetcher) -> Self {
        Self {
            fetcher,
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn slot(&self, url: &Url) -> Arc<tokio::sync::Mutex<Option<Entry>>> {
        let mut entries = self.entries.lock().unwrap();
        if !entries.contains_key(url) && entries.len() >= MAX_CACHE_ENTRIES {
            // Only drop slots which are not in use
            entries.retain(|_, slot| Arc::strong_count(slot) > 1);
            if entries.len() >= MAX_CACHE_ENTRIES {
                entries.clear();
            }
        }
        entries.entry(url.clone()).or_default().clone()
    }

    pub async fn get(&self, url: &Url) -> Result<Arc<Schema>, String> {
        let slot = self.slot(url);
        let mut guard = slot.lock().await;

        if let Some(entry) = guard.as_ref() {
            if let Some(schema) = &entry.schema
                && entry.fetched_at.elapsed() < CACHE_TTL
            {
                return Ok(schema.clone());
            }

            if let Some((error, at)) = &entry.error
                && at.elapsed() < ERROR_TTL
            {
                return Err(error.clone());
            }
        }

        let previous = guard.take();
        let etag = previous.as_ref().and_then(|e| e.etag.clone());
        let stale = previous.as_ref().and_then(|e| e.schema.clone());
        let etag = stale.as_ref().and(etag);

        match self.fetcher.fetch(url, etag.as_deref()).await {
            Ok(Fetched::Schema { schema, etag }) => {
                let schema = Arc::new(schema);
                *guard = Some(Entry {
                    schema: Some(schema.clone()),
                    etag,
                    fetched_at: Instant::now(),
                    error: None,
                });
                Ok(schema)
            }
            Ok(Fetched::NotModified) => {
                let schema = stale.expect("a conditional request needs a cached schema");
                *guard = Some(Entry {
                    schema: Some(schema.clone()),
                    etag,
                    fetched_at: Instant::now(),
                    error: None,
                });
                Ok(schema)
            }
            Err(error) => {
                // Keep using an outdated schema (offline), but remember the failure briefly
                let result = match &stale {
                    Some(schema) => Ok(schema.clone()),
                    None => Err(error.clone()),
                };
                *guard = Some(Entry {
                    schema: stale,
                    etag,
                    fetched_at: Instant::now() - CACHE_TTL,
                    error: Some((error, Instant::now())),
                });
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const SCHEMA: &str = "
        [toml-schema]
        version = \"1.0.0\"

        [elements]
        name = { type = \"string\" }
    ";

    #[test]
    fn check_url_rules() {
        let check = |s: &str| validate_url(&Url::parse(s).unwrap(), false);

        assert!(check("https://example.com/a.tosd").is_ok());
        assert!(check("http://example.com/a.tosd").is_err());
        assert!(check("file:///etc/passwd").is_err());
        assert!(check("https://user:pw@example.com/a.tosd").is_err());
    }

    // Serve a canned response for every connection
    async fn serve(response: Vec<u8>) -> Url {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let response = response.clone();

                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = socket.read(&mut buf).await;
                    let _ = socket.write_all(&response).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        Url::parse(&format!("http://127.0.0.1:{port}/s.tosd")).unwrap()
    }

    fn http(status: &str, headers: &str, body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    // Plain http is allowed here, so the tests can use a local server
    fn test_cache() -> RemoteSchemas {
        RemoteSchemas::new(Fetcher::new(true))
    }

    #[tokio::test]
    async fn fetches_valid_schema() {
        let url = serve(http("200 OK", "ETag: \"1\"\r\n", SCHEMA)).await;

        assert!(test_cache().get(&url).await.is_ok());
    }

    #[tokio::test]
    async fn rejects_oversized_body() {
        let body = "#".repeat(MAX_BODY_BYTES + 1);
        let url = serve(http("200 OK", "", &body)).await;
        let err = test_cache().get(&url).await.err().unwrap();

        assert!(err.contains("exceeds"), "{err}");
    }

    #[tokio::test]
    async fn rejects_non_toml_and_error_status() {
        let url = serve(http("200 OK", "", "not = [toml")).await;

        assert!(test_cache().get(&url).await.is_err());

        let url = serve(http("404 Not Found", "", "secret body")).await;
        let err = test_cache().get(&url).await.err().unwrap();

        assert!(err.contains("404") && !err.contains("secret"), "{err}");
    }

    #[tokio::test]
    async fn caches_between_requests() {
        let url = serve(http("200 OK", "", SCHEMA)).await;
        let cache = test_cache();
        let first = cache.get(&url).await.unwrap();
        let second = cache.get(&url).await.unwrap();

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[tokio::test]
    async fn rejects_redirect_to_another_host() {
        let target = serve(http("200 OK", "", SCHEMA)).await;
        let other_host = target.as_str().replace("127.0.0.1", "localhost");
        let redirect = serve(http(
            "302 Found",
            &format!("Location: {other_host}\r\n"),
            "",
        ))
        .await;

        assert!(test_cache().get(&redirect).await.is_err());
    }

    #[tokio::test]
    async fn follows_redirect_on_the_same_host() {
        let target = serve(http("200 OK", "", SCHEMA)).await;
        let redirect = serve(http("302 Found", &format!("Location: {target}\r\n"), "")).await;

        assert!(test_cache().get(&redirect).await.is_ok());
    }

    #[tokio::test]
    async fn production_mode_rejects_http_urls() {
        let url = serve(http("200 OK", "", SCHEMA)).await;

        assert!(Fetcher::new(false).fetch(&url, None).await.is_err());
    }
}
