//! Origin-keyed HTTP Cookie Jar (RFC 6265 subset).

use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub host_only: bool,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,
    pub expires_at: Option<u64>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn default_path(url: &Url) -> String {
    let p = url.path();
    if !p.starts_with('/') || p == "/" {
        return "/".to_string();
    }
    match p.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(idx) => p[..idx].to_string(),
    }
}

fn domain_matches(host: &str, cookie_domain: &str, host_only: bool) -> bool {
    let h = host.to_ascii_lowercase();
    let d = cookie_domain.to_ascii_lowercase();
    if host_only {
        return h == d;
    }
    h == d || (h.ends_with(&d) && h.as_bytes().get(h.len() - d.len() - 1) == Some(&b'.'))
}

fn path_matches(req_path: &str, cookie_path: &str) -> bool {
    if req_path == cookie_path {
        return true;
    }
    if req_path.starts_with(cookie_path) {
        return cookie_path.ends_with('/')
            || req_path.as_bytes().get(cookie_path.len()) == Some(&b'/');
    }
    false
}

#[derive(Debug, Default)]
pub struct CookieJar {
    cookies: Vec<Cookie>,
}

impl CookieJar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    pub fn len(&self) -> usize {
        self.cookies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    pub fn store_from_headers(&mut self, url: &Url, headers: &[&str]) {
        let Some(host) = url.host_str() else {
            return;
        };
        let host_lower = host.to_ascii_lowercase();
        let now = now_secs();

        for raw in headers {
            let mut parts = raw.split(';');
            let Some(nv) = parts.next() else {
                continue;
            };
            let Some((name, value)) = nv.split_once('=') else {
                continue;
            };
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let value = value.trim().to_string();

            let mut domain = host_lower.clone();
            let mut host_only = true;
            let mut path = default_path(url);
            let mut secure = false;
            let mut http_only = false;
            let mut same_site = SameSite::Lax;
            let mut expires_at = None;

            for attr in parts {
                let attr = attr.trim();
                if attr.eq_ignore_ascii_case("secure") {
                    secure = true;
                } else if attr.eq_ignore_ascii_case("httponly") {
                    http_only = true;
                } else if let Some((k, v)) = attr.split_once('=') {
                    let k = k.trim();
                    let v = v.trim();
                    if k.eq_ignore_ascii_case("domain") && !v.is_empty() {
                        let d = v.trim_start_matches('.').to_ascii_lowercase();
                        if domain_matches(&host_lower, &d, false) {
                            domain = d;
                            host_only = false;
                        } else {
                            // Reject cross-domain cookie spoofing.
                            domain.clear();
                            break;
                        }
                    } else if k.eq_ignore_ascii_case("path") && v.starts_with('/') {
                        path = v.to_string();
                    } else if k.eq_ignore_ascii_case("max-age")
                        && let Ok(secs) = v.parse::<i64>()
                    {
                        expires_at = Some(if secs <= 0 {
                            0
                        } else {
                            now.saturating_add(secs as u64)
                        });
                    } else if k.eq_ignore_ascii_case("samesite") {
                        same_site = if v.eq_ignore_ascii_case("strict") {
                            SameSite::Strict
                        } else if v.eq_ignore_ascii_case("none") {
                            SameSite::None
                        } else {
                            SameSite::Lax
                        };
                    }
                }
            }

            if domain.is_empty() {
                continue;
            }
            // SameSite=None requires Secure per RFC6265bis.
            if same_site == SameSite::None && !secure {
                continue;
            }

            self.cookies.retain(|c| {
                !(c.name == name && c.domain == domain && c.path == path)
            });

            if expires_at != Some(0) {
                self.cookies.push(Cookie {
                    name: name.to_string(),
                    value,
                    domain,
                    host_only,
                    path,
                    secure,
                    http_only,
                    same_site,
                    expires_at,
                });
            }
        }
    }

    pub fn cookie_header_for(
        &mut self,
        url: &Url,
        is_same_site: bool,
        is_top_level_nav: bool,
        from_script: bool,
    ) -> Option<String> {
        let host = url.host_str()?;
        let is_https = url.scheme() == "https";
        let req_path = url.path();
        let now = now_secs();

        self.cookies.retain(|c| c.expires_at.is_none_or(|exp| exp > now));

        let pairs: Vec<String> = self
            .cookies
            .iter()
            .filter(|c| {
                domain_matches(host, &c.domain, c.host_only)
                    && path_matches(req_path, &c.path)
                    && (!c.secure || is_https)
                    && (!from_script || !c.http_only)
                    && match c.same_site {
                        SameSite::Strict => is_same_site,
                        SameSite::Lax => is_same_site || is_top_level_nav,
                        SameSite::None => c.secure,
                    }
            })
            .map(|c| format!("{}={}", c.name, c.value))
            .collect();

        if pairs.is_empty() {
            None
        } else {
            Some(pairs.join("; "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_secure_httponly_domain_path_and_samesite() {
        let mut jar = CookieJar::new();
        let https_url = Url::parse("https://app.example.com/api/v1").unwrap();
        jar.store_from_headers(
            &https_url,
            &[
                "sid=abc; Secure; HttpOnly; SameSite=Strict; Path=/api",
                "pref=dark; Domain=example.com; Path=/; Max-Age=3600",
                "bad=1; Domain=evil.com",
                "insecure_none=1; SameSite=None",
            ],
        );
        assert_eq!(jar.len(), 2);

        // Same-site HTTPS request gets both.
        assert_eq!(
            jar.cookie_header_for(&https_url, true, false, false).as_deref(),
            Some("sid=abc; pref=dark")
        );
        // Script access excludes HttpOnly `sid`.
        assert_eq!(
            jar.cookie_header_for(&https_url, true, false, true).as_deref(),
            Some("pref=dark")
        );
        // Cross-site subresource request excludes both Strict and Lax.
        assert_eq!(
            jar.cookie_header_for(&https_url, false, false, false).as_deref(),
            None
        );
        // Cross-site top-level navigation excludes Strict `sid` but sends Lax `pref`.
        assert_eq!(
            jar.cookie_header_for(&https_url, false, true, false).as_deref(),
            Some("pref=dark")
        );
        // Plain HTTP excludes Secure `sid` but keeps `pref` on subdomain.
        let http_sub = Url::parse("http://sub.example.com/home").unwrap();
        assert_eq!(
            jar.cookie_header_for(&http_sub, true, true, false).as_deref(),
            Some("pref=dark")
        );
    }
}
