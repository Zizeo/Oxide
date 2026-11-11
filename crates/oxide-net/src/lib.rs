//! Network engine and resource fetcher for Oxide.

pub mod security;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
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

#[derive(Debug, Clone)]
struct CachedEntry {
    response: HttpResponse,
    #[allow(dead_code)]
    cached_at: Instant,
}

#[derive(Clone)]
pub struct NetworkEngine {
    client: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, CachedEntry>>>,
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
        }
    }

    /// Fetches a URL asynchronously.
    /// Supports `http://`, `https://`, `data:`, `file://`, and `oxide://` schemes.
    pub async fn fetch(&self, raw_input: &str) -> Result<HttpResponse, NetError> {
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

        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(&url_str) {
                debug!("Cache hit for {}", url_str);
                return Ok(entry.response.clone());
            }
        }

        let response = match url.scheme() {
            "data" => self.handle_data_url(&url, start_time)?,
            "http" | "https" => self.handle_http_url(url, start_time).await?,
            "file" => self.handle_file_url(&url, start_time).await?,
            other => return Err(NetError::UnsupportedScheme(other.to_string())),
        };

        // Cache successful HTTP/file responses
        if response.status == 200 && (url_str.starts_with("http") || url_str.starts_with("file")) {
            let mut cache = self.cache.write().await;
            cache.insert(
                url_str,
                CachedEntry {
                    response: response.clone(),
                    cached_at: Instant::now(),
                },
            );
        }

        Ok(response)
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

    async fn handle_http_url(&self, url: Url, start_time: Instant) -> Result<HttpResponse, NetError> {
        info!("Fetching HTTP/HTTPS resource: {}", url);
        let resp = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(|e| NetError::RequestFailed(e.to_string()))?;

        let status = resp.status().as_u16();
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

        let body = resp
            .text()
            .await
            .map_err(|e| NetError::RequestFailed(e.to_string()))?;

        Ok(HttpResponse {
            url: url.to_string(),
            status,
            headers,
            body,
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

