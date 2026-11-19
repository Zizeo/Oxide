//! Network engine and resource fetcher for Oxide.

pub mod cookies;
pub mod security;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info};
use url::Url;

#[derive(Error, Debug, Clone)]
pub enum NetError {
    #[error("Invalid URL format: {0}")]
    InvalidUrl(String),

    #[error("Network request failed: {0}")]
    RequestFailed(String),

    #[error("HTTP error: status code {0}")]
    HttpStatus(u16),

    #[error("Data URL decode error: {0}")]
    DataUrlError(String),

    #[error("File read error: {0}")]
    FileError(String),

    #[error("Unsupported scheme: {0}")]
    UnsupportedScheme(String),
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub url: String,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub content_type: String,
    pub duration_ms: u128,
}

/// Returns `true` when HTTP response headers indicate a file download rather
/// than a renderable document (`Content-Disposition: attachment` or binary
/// archive/installer MIME types).
#[must_use]
pub fn is_download_response(content_type: &str, headers: &HashMap<String, String>) -> bool {
    if headers.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("content-disposition")
            && v.to_ascii_lowercase().contains("attachment")
    }) {
        return true;
    }
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    matches!(
        mime.as_str(),
        "application/octet-stream"
            | "application/zip"
            | "application/x-zip-compressed"
            | "application/x-tar"
            | "application/gzip"
            | "application/x-gzip"
            | "application/x-7z-compressed"
            | "application/x-rar-compressed"
            | "application/x-msdownload"
            | "application/vnd.microsoft.portable-executable"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl HttpMethod {
    fn to_reqwest(self) -> reqwest::Method {
        match self {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Delete => reqwest::Method::DELETE,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Head => reqwest::Method::HEAD,
            HttpMethod::Options => reqwest::Method::OPTIONS,
        }
    }
}

#[derive(Debug, Clone)]
pub enum RequestBody {
    None,
    FormUrlEncoded(Vec<(String, String)>),
    Json(String),
    Multipart(Vec<(String, String, Vec<u8>)>),
    Raw { content_type: String, body: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityMetadata {
    pub is_https: bool,
    pub hsts: bool,
    pub csp_present: bool,
    pub summary: String,
}

impl HttpResponse {
    #[must_use]
    pub fn security_metadata(&self) -> SecurityMetadata {
        let is_https = self.url.starts_with("https://");
        let hsts = self.headers.contains_key("strict-transport-security");
        let csp_present = self.headers.contains_key("content-security-policy");
        let summary = if is_https {
            format!(
                "TLS (rustls){}{}",
                if hsts { " • HSTS" } else { "" },
                if csp_present { " • CSP" } else { "" }
            )
        } else {
            "Unencrypted".to_string()
        };
        SecurityMetadata {
            is_https,
            hsts,
            csp_present,
            summary,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct CacheDirectives {
    no_store: bool,
    no_cache: bool,
    max_age: Option<u64>,
}

impl CacheDirectives {
    fn parse(header: &str) -> Self {
        let mut out = Self::default();
        for part in header.split(',') {
            let p = part.trim().to_ascii_lowercase();
            if p == "no-store" {
                out.no_store = true;
            } else if p == "no-cache" {
                out.no_cache = true;
            } else if let Some(val) = p.strip_prefix("max-age=")
                && let Ok(secs) = val.trim().parse::<u64>()
            {
                out.max_age = Some(secs);
            }
        }
        out
    }
}

#[derive(Debug, Clone)]
struct CachedEntry {
    response: HttpResponse,
    cached_at: SystemTime,
    max_age: Option<u64>,
    no_cache: bool,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl CachedEntry {
    fn is_fresh(&self) -> bool {
        if self.no_cache {
            return false;
        }
        let age = SystemTime::now()
            .duration_since(self.cached_at)
            .map(|d| d.as_secs())
            .unwrap_or(u64::MAX);
        let limit = self.max_age.unwrap_or(60);
        age < limit
    }
}

#[derive(Clone)]
pub struct NetworkEngine {
    client: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, CachedEntry>>>,
    cookies: Arc<RwLock<cookies::CookieJar>>,
}

impl Default for NetworkEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkEngine {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("OxideBrowser/0.1 (X11; Linux x86_64; rv:1.0) Oxide/2026")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
            cookies: Arc::new(RwLock::new(cookies::CookieJar::new())),
        }
    }

    pub fn cookies(&self) -> Arc<RwLock<cookies::CookieJar>> {
        self.cookies.clone()
    }

    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }

    /// Streams `url` to `save_path`, honoring `paused`/`cancelled` flags and
    /// invoking `on_progress(bytes_received, total_bytes)` per chunk.
    pub async fn download_to_file<F>(
        &self,
        url: &str,
        save_path: &std::path::Path,
        paused: &std::sync::atomic::AtomicBool,
        cancelled: &std::sync::atomic::AtomicBool,
        mut on_progress: F,
    ) -> Result<(), String>
    where
        F: FnMut(u64, Option<u64>),
    {
        use std::io::Write;
        use std::sync::atomic::Ordering;

        let mut resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let total_bytes = resp.content_length();
        let mut file = std::fs::File::create(save_path).map_err(|e| e.to_string())?;
        let mut bytes_received: u64 = 0;

        while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
            while paused.load(Ordering::Relaxed) {
                if cancelled.load(Ordering::Relaxed) {
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            if cancelled.load(Ordering::Relaxed) {
                return Ok(());
            }
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            bytes_received += chunk.len() as u64;
            on_progress(bytes_received, total_bytes);
        }
        Ok(())
    }

    /// Fetches a URL asynchronously.
    /// Supports `http://`, `https://`, `data:`, `file://`, and `oxide://` schemes.
    pub async fn fetch(&self, raw_input: &str) -> Result<HttpResponse, NetError> {
        self.fetch_with_options(raw_input, false).await
    }

    pub async fn fetch_with_options(
        &self,
        raw_input: &str,
        bypass_cache: bool,
    ) -> Result<HttpResponse, NetError> {
        self.request(HttpMethod::Get, raw_input, HashMap::new(), RequestBody::None, bypass_cache)
            .await
    }

    pub async fn request(
        &self,
        method: HttpMethod,
        raw_input: &str,
        extra_headers: HashMap<String, String>,
        body: RequestBody,
        bypass_cache: bool,
    ) -> Result<HttpResponse, NetError> {
        let trimmed = raw_input.trim();
        let start_time = Instant::now();

        // Check if raw input is an internal scheme like `oxide://`
        if let Some(rest) = trimmed.strip_prefix("oxide://") {
            return self.handle_internal_scheme(rest, start_time);
        }

        if trimmed == "about:blank" {
            return Ok(HttpResponse {
                url: "about:blank".to_string(),
                status: 200,
                headers: HashMap::new(),
                body: String::new(),
                content_type: "text/html".to_string(),
                duration_ms: start_time.elapsed().as_millis(),
            });
        }

        // Parse as URL or prepend https:// if it looks like a domain name
        let url = if trimmed.starts_with("data:") || trimmed.contains("://") {
            Url::parse(trimmed).map_err(|e| NetError::InvalidUrl(e.to_string()))?
        } else if trimmed.contains(' ') {
            // Search query
            let query = percent_encoding::utf8_percent_encode(trimmed, percent_encoding::NON_ALPHANUMERIC);
            Url::parse(&format!("https://html.duckduckgo.com/html/?q={query}"))
                .map_err(|e| NetError::InvalidUrl(e.to_string()))?
        } else if !trimmed.contains('.') {
            // E.g. "youtube" -> "https://www.youtube.com"
            Url::parse(&format!("https://www.{trimmed}.com"))
                .map_err(|e| NetError::InvalidUrl(e.to_string()))?
        } else {
            Url::parse(&format!("https://{trimmed}"))
                .map_err(|e| NetError::InvalidUrl(e.to_string()))?
        };

        let url_str = url.as_str().to_string();
        let is_get = method == HttpMethod::Get;

        let cached_entry = if is_get && !bypass_cache {
            let cache = self.cache.read().await;
            cache.get(&url_str).cloned()
        } else {
            None
        };

        if let Some(entry) = &cached_entry
            && entry.is_fresh()
        {
            debug!("Cache hit for {}", url_str);
            return Ok(entry.response.clone());
        }

        let response = match url.scheme() {
            "data" => self.handle_data_url(&url, start_time)?,
            "http" | "https" => {
                self.handle_http_request(
                    method,
                    url,
                    extra_headers,
                    body,
                    cached_entry.as_ref(),
                    start_time,
                )
                .await?
            }
            "file" => self.handle_file_url(&url, start_time).await?,
            other => return Err(NetError::UnsupportedScheme(other.to_string())),
        };

        if is_get
            && response.status == 200
            && (url_str.starts_with("http") || url_str.starts_with("file"))
        {
            let cc = response
                .headers
                .get("cache-control")
                .map(|s| CacheDirectives::parse(s))
                .unwrap_or_default();
            if !cc.no_store {
                let mut cache = self.cache.write().await;
                cache.insert(
                    url_str,
                    CachedEntry {
                        etag: response.headers.get("etag").cloned(),
                        last_modified: response.headers.get("last-modified").cloned(),
                        max_age: cc.max_age,
                        no_cache: cc.no_cache,
                        response: response.clone(),
                        cached_at: SystemTime::now(),
                    },
                );
            }
        }

        Ok(response)
    }

    /// Concurrently fetches subresources (stylesheets, scripts, images, fonts)
    /// with bounded parallelism (up to 8 in-flight requests).
    pub async fn fetch_subresources(&self, urls: Vec<String>) -> Vec<(String, Result<Vec<u8>, NetError>)> {
        let sem = Arc::new(tokio::sync::Semaphore::new(8));
        let mut set = tokio::task::JoinSet::new();
        for url in urls {
            let engine = self.clone();
            let permit = sem.clone();
            set.spawn(async move {
                let _p = permit.acquire().await;
                let res = engine.fetch_bytes(&url, 8 * 1024 * 1024).await;
                (url, res)
            });
        }
        let mut out = Vec::new();
        while let Some(res) = set.join_next().await {
            if let Ok(item) = res {
                out.push(item);
            }
        }
        out
    }

    /// Fetches a URL and returns the body exactly as it arrived.
    ///
    /// [`fetch`](Self::fetch) decodes the body as lossy UTF-8, which destroys
    /// binary resources. Favicons are the one thing the browser needs byte for
    /// byte, so they get their own entry point.
    pub async fn fetch_bytes(
        &self,
        raw_url: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>, NetError> {
        let url = Url::parse(raw_url.trim()).map_err(|e| NetError::InvalidUrl(e.to_string()))?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(NetError::UnsupportedScheme(url.scheme().to_string()));
        }

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| NetError::RequestFailed(e.to_string()))?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(NetError::HttpStatus(status));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| NetError::RequestFailed(e.to_string()))?;
        if bytes.len() > max_bytes {
            // A favicon is a few kilobytes; anything larger is not one, and
            // handing it to the decoder would only waste memory.
            return Err(NetError::RequestFailed(format!(
                "{} byte response exceeds the {max_bytes} byte limit",
                bytes.len()
            )));
        }
        Ok(bytes.to_vec())
    }

    async fn handle_http_request(
        &self,
        method: HttpMethod,
        url: Url,
        extra_headers: HashMap<String, String>,
        body: RequestBody,
        stale_entry: Option<&CachedEntry>,
        start_time: Instant,
    ) -> Result<HttpResponse, NetError> {
        info!("Fetching HTTP/HTTPS resource: {:?} {}", method, url);
        let mut req = self.client.request(method.to_reqwest(), url.clone());

        if let Some(cookie_hdr) = self.cookies.write().await.cookie_header_for(&url, true, true, false) {
            req = req.header("Cookie", cookie_hdr);
        }
        if let Some(entry) = stale_entry {
            if let Some(etag) = &entry.etag {
                req = req.header("If-None-Match", etag);
            }
            if let Some(lm) = &entry.last_modified {
                req = req.header("If-Modified-Since", lm);
            }
        }
        for (k, v) in extra_headers {
            req = req.header(k, v);
        }
        req = match body {
            RequestBody::None => req,
            RequestBody::FormUrlEncoded(pairs) => req.form(&pairs),
            RequestBody::Json(json) => req
                .header("Content-Type", "application/json")
                .body(json),
            RequestBody::Multipart(parts) => {
                let boundary = "----OxideBoundary7MA4YWxkTrZu0gW";
                let mut payload = Vec::new();
                for (name, filename, bytes) in parts {
                    payload.extend_from_slice(
                        format!(
                            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                        )
                        .as_bytes(),
                    );
                    payload.extend_from_slice(&bytes);
                    payload.extend_from_slice(b"\r\n");
                }
                payload.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
                req.header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
                    .body(payload)
            }
            RequestBody::Raw { content_type, body } => {
                req.header("Content-Type", content_type).body(body)
            }
        };

        let resp = req
            .send()
            .await
            .map_err(|e| NetError::RequestFailed(e.to_string()))?;

        let status = resp.status().as_u16();
        let set_cookies: Vec<String> = resp
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok().map(str::to_string))
            .collect();
        if !set_cookies.is_empty() {
            let refs: Vec<&str> = set_cookies.iter().map(String::as_str).collect();
            self.cookies.write().await.store_from_headers(&url, &refs);
        }

        if status == 304
            && let Some(entry) = stale_entry
        {
            return Ok(entry.response.clone());
        }

        let mut headers = HashMap::new();
        for (k, v) in resp.headers().iter() {
            if let Ok(val) = v.to_str() {
                headers.insert(k.as_str().to_ascii_lowercase(), val.to_string());
            }
        }

        let content_type = headers
            .get("content-type")
            .cloned()
            .unwrap_or_else(|| "text/html; charset=utf-8".to_string());

        let body_text = if method == HttpMethod::Head {
            String::new()
        } else {
            resp.text()
                .await
                .map_err(|e| NetError::RequestFailed(e.to_string()))?
        };

        Ok(HttpResponse {
            url: url.to_string(),
            status,
            headers,
            body: body_text,
            content_type,
            duration_ms: start_time.elapsed().as_millis(),
        })
    }

    fn handle_data_url(&self, url: &Url, start_time: Instant) -> Result<HttpResponse, NetError> {
        let path = url.path();
        // data:[<mediatype>][;base64],<data>
        let (metadata, data) = path
            .split_once(',')
            .ok_or_else(|| NetError::DataUrlError("Missing comma in data URL".to_string()))?;

        let is_base64 = metadata.ends_with(";base64");
        let content_type = if is_base64 {
            metadata.trim_end_matches(";base64")
        } else if metadata.is_empty() {
            "text/plain;charset=US-ASCII"
        } else {
            metadata
        };

        let body = if is_base64 {
            // Simple base64 decoding or url-decode
            let decoded_bytes = simple_base64_decode(data.trim())
                .map_err(|e| NetError::DataUrlError(format!("Base64 decode error: {}", e)))?;
            String::from_utf8_lossy(&decoded_bytes).to_string()
        } else {
            // Percent-decoded string
            percent_encoding::percent_decode_str(data)
                .decode_utf8_lossy()
                .to_string()
        };

        Ok(HttpResponse {
            url: url.to_string(),
            status: 200,
            headers: HashMap::new(),
            body,
            content_type: content_type.to_string(),
            duration_ms: start_time.elapsed().as_millis(),
        })
    }

    async fn handle_file_url(&self, url: &Url, start_time: Instant) -> Result<HttpResponse, NetError> {
        let file_path = url
            .to_file_path()
            .map_err(|_| NetError::FileError("Invalid local file path".to_string()))?;

        let content = tokio::fs::read_to_string(&file_path)
            .await
            .map_err(|e| NetError::FileError(e.to_string()))?;

        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("html");

        let content_type = match ext {
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "js" => "application/javascript",
            "json" => "application/json",
            "txt" => "text/plain",
            _ => "text/html",
        };

        Ok(HttpResponse {
            url: url.to_string(),
            status: 200,
            headers: HashMap::new(),
            body: content,
            content_type: content_type.to_string(),
            duration_ms: start_time.elapsed().as_millis(),
        })
    }

    fn handle_internal_scheme(&self, page: &str, start_time: Instant) -> Result<HttpResponse, NetError> {
        let (title, content) = match page {
            "welcome" | "" => (
                "Welcome to Oxide",
                r#"<!DOCTYPE html>
<html>
<head>
    <title>Welcome to Oxide</title>
    <style>
        body { font-family: sans-serif; background: #0f172a; color: #f8fafc; padding: 40px; }
        h1 { color: #38bdf8; font-size: 36px; }
        p { font-size: 18px; line-height: 1.6; }
        .card { background: #1e293b; border-radius: 8px; padding: 20px; margin-top: 20px; }
        a { color: #60a5fa; text-decoration: none; }
    </style>
</head>
<body>
    <h1>Welcome to Oxide Browser</h1>
    <p>A fast, safe, modular web browser written in 100% idiomatic Rust.</p>
    <div class="card">
        <h3>Architecture Highlights:</h3>
        <ul>
            <li><strong>GPU Acceleration:</strong> WGPU + Vello compute pipeline</li>
            <li><strong>Layout Engine:</strong> Taffy (Flexbox / CSS layout)</li>
            <li><strong>DOM Arena:</strong> SlotMap generational node index</li>
            <li><strong>JavaScript:</strong> Boa JS engine integration</li>
        </ul>
    </div>
</body>
</html>"#,
            ),
            "version" => (
                "Oxide Version",
                "<!DOCTYPE html><html><body><h1>Oxide 0.1.0 (Developer Preview)</h1><p>Running on Rust 1.85+ with WGPU 24.</p></body></html>",
            ),
            _ => (
                "404 Not Found",
                "<!DOCTYPE html><html><body><h1>404 Internal Page Not Found</h1></body></html>",
            ),
        };

        let mut headers = HashMap::new();
        headers.insert("title".to_string(), title.to_string());

        Ok(HttpResponse {
            url: format!("oxide://{}", page),
            status: 200,
            headers,
            body: content.to_string(),
            content_type: "text/html".to_string(),
            duration_ms: start_time.elapsed().as_millis(),
        })
    }
}

fn simple_base64_decode(input: &str) -> Result<Vec<u8>, String> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut decode_map = [255u8; 256];
    for (i, &b) in TABLE.iter().enumerate() {
        decode_map[b as usize] = i as u8;
    }

    let input_bytes = input.as_bytes();
    let mut output = Vec::with_capacity((input_bytes.len() * 3) / 4);
    let mut buf = 0u32;
    let mut bits = 0;

    for &b in input_bytes {
        if b == b'=' || b == b'\r' || b == b'\n' || b == b' ' {
            continue;
        }
        let val = decode_map[b as usize];
        if val == 255 {
            return Err(format!("Invalid base64 byte: {}", b as char));
        }
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_about_blank() {
        let net = NetworkEngine::new();
        let resp = net.fetch("about:blank").await.expect("Failed about:blank");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.url, "about:blank");
        assert!(resp.body.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_oxide_welcome() {
        let net = NetworkEngine::new();
        let resp = net.fetch("oxide://welcome").await.expect("Failed oxide://welcome");
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("Welcome to Oxide Browser"));
        assert_eq!(resp.headers.get("title").map(|s| s.as_str()), Some("Welcome to Oxide"));
    }

    #[tokio::test]
    async fn test_fetch_data_url_plain() {
        let net = NetworkEngine::new();
        let resp = net
            .fetch("data:text/html,<h1>Hello%20World</h1>")
            .await
            .expect("Failed data url");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, "<h1>Hello World</h1>");
        assert_eq!(resp.content_type, "text/html");
    }

    #[tokio::test]
    async fn test_fetch_data_url_base64() {
        let net = NetworkEngine::new();
        // "<h1>Oxide</h1>" in base64 is "PGgxPk94aWRlPC9oMT4="
        let resp = net
            .fetch("data:text/html;base64,PGgxPk94aWRlPC9oMT4=")
            .await
            .expect("Failed base64 data url");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, "<h1>Oxide</h1>");
        assert_eq!(resp.content_type, "text/html");
    }

    #[tokio::test]
    async fn test_fetch_invalid_scheme() {
        let net = NetworkEngine::new();
        let err = net.fetch("ftp://example.com").await.unwrap_err();
        match err {
            NetError::UnsupportedScheme(scheme) => assert_eq!(scheme, "ftp"),
            other => panic!("Unexpected error: {:?}", other),
        }
    }
}

