//! Security policy enforcement and Google Search omnibox resolution.

use url::{Host, Url};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationDecision {
    Allow(String),
    UpgradeToHttps(String),
    Block(String),
}

#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    pub https_only: bool,
    pub block_private_networks: bool,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            https_only: true,
            block_private_networks: true,
        }
    }
}

impl SecurityPolicy {
    #[must_use]
    pub fn validate_navigation(&self, raw_url: &str) -> NavigationDecision {
        let trimmed = raw_url.trim();
        if trimmed.is_empty() {
            return NavigationDecision::Block("Empty URL".to_string());
        }

        if trimmed.eq_ignore_ascii_case("about:blank") || trimmed.starts_with("oxide://") {
            return NavigationDecision::Allow(trimmed.to_string());
        }

        let parsed = match Url::parse(trimmed) {
            Ok(u) => u,
            Err(e) => return NavigationDecision::Block(format!("Malformed URL: {e}")),
        };

        let scheme = parsed.scheme();
        match scheme {
            "https" | "http" => {}
            "oxide" | "about" => return NavigationDecision::Allow(parsed.to_string()),
            other => {
                return NavigationDecision::Block(format!(
                    "Blocked unsafe URL scheme '{other}:' by Oxide Security Policy"
                ));
            }
        }

        if self.block_private_networks {
            if let Some(host) = parsed.host() {
                if is_private_or_loopback_host(&host) {
                    return NavigationDecision::Block(format!(
                        "Blocked private/local network target '{host}' (SSRF protection active)"
                    ));
                }
            } else {
                return NavigationDecision::Block("Missing host in HTTP/HTTPS URL".to_string());
            }
        }

        if scheme == "http" && self.https_only {
            let mut upgraded = parsed;
            if upgraded.set_scheme("https").is_ok() {
                return NavigationDecision::UpgradeToHttps(upgraded.to_string());
            }
            return NavigationDecision::Block("Failed to upgrade HTTP URL to HTTPS".to_string());
        }

        NavigationDecision::Allow(parsed.to_string())
    }

    /// Validates a navigation inside a WebView, permitting a single internal `data:text/html` load
    /// only when `allow_internal_html` was explicitly armed by the browser for `oxide://welcome`.
    pub fn validate_webview_navigation(
        &self,
        raw_url: &str,
        allow_internal_html: &std::sync::atomic::AtomicBool,
    ) -> NavigationDecision {
        use std::sync::atomic::Ordering;
        let trimmed = raw_url.trim();
        if trimmed.starts_with("data:text/html")
            && allow_internal_html.swap(false, Ordering::SeqCst)
        {
            return NavigationDecision::Allow("oxide://welcome".to_string());
        }
        self.validate_navigation(trimmed)
    }
}

fn is_private_or_loopback_host(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => {
            let d = domain.to_ascii_lowercase();
            d == "localhost"
                || d.ends_with(".localhost")
                || d.ends_with(".local")
                || d.ends_with(".internal")
        }
        Host::Ipv4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
        }
        Host::Ipv6(ip) => {
            let seg0 = ip.segments()[0];
            let is_unique_local = (seg0 & 0xfe00) == 0xfc00;
            let is_link_local = (seg0 & 0xffc0) == 0xfe80;
            ip.is_loopback() || ip.is_unspecified() || is_unique_local || is_link_local
        }
    }
}

/// Resolves user input in the Omnibox into either a normalized URL or a Google Search URL.
#[must_use]
pub fn resolve_omnibox_input(input: &str) -> String {
    resolve_omnibox_input_with_template(input, "https://www.google.com/search?q={query}")
}

/// Resolves user input in the Omnibox using a custom search URL template (`{query}`).
#[must_use]
pub fn resolve_omnibox_input_with_template(input: &str, search_template: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "oxide://welcome".to_string();
    }

    if trimmed.eq_ignore_ascii_case("about:blank") || trimmed.starts_with("oxide://") {
        return trimmed.to_string();
    }

    // If input has an explicit scheme (e.g. https://, http://, file://, javascript:)
    if !trimmed.contains(' ')
        && let Some((scheme_candidate, _)) = trimmed.split_once(':')
        && scheme_candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        && !scheme_candidate.eq_ignore_ascii_case("localhost")
    {
        // Only treat as explicit scheme if the part after ':' is not just a port number
        let after_colon = &trimmed[scheme_candidate.len() + 1..];
        if !after_colon.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(u) = Url::parse(trimmed) {
                return u.to_string();
            }
            return trimmed.to_string();
        }
    }

    // Check for localhost or localhost:port
    if (trimmed.eq_ignore_ascii_case("localhost")
        || trimmed
            .to_ascii_lowercase()
            .starts_with("localhost:")
        || trimmed.starts_with("localhost/"))
        && let Ok(u) = Url::parse(&format!("http://{trimmed}"))
    {
        return u.to_string();
    }

    // Looks like a domain name or IP if it has no spaces and contains a dot (e.g. "google.com")
    if !trimmed.contains(' ')
        && trimmed.contains('.')
        && let Ok(u) = Url::parse(&format!("https://{trimmed}"))
    {
        return u.to_string();
    }

    // Otherwise, route to configured search engine template
    let encoded = percent_encoding::utf8_percent_encode(trimmed, percent_encoding::NON_ALPHANUMERIC);
    let template = if search_template.contains("{query}") {
        search_template
    } else {
        "https://www.google.com/search?q={query}"
    };
    template.replace("{query}", &encoded.to_string())
}

/// Built-in HTML for `oxide://welcome` featuring a Google Search form and quick links.
#[must_use]
pub fn welcome_page_html() -> &'static str {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>New Tab</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: "Segoe UI", system-ui, -apple-system, sans-serif;
            background: #202124;
            color: #e8eaed;
            min-height: 100vh;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            padding: 32px 24px 72px;
        }
        .container {
            max-width: 584px;
            width: 100%;
            text-align: center;
        }
        .brand {
            font-size: 56px;
            font-weight: 600;
            letter-spacing: -0.03em;
            color: #e8eaed;
            margin-bottom: 32px;
            user-select: none;
        }
        .search-form {
            display: flex;
            align-items: center;
            width: 100%;
            height: 46px;
            background: #303134;
            border: 1px solid #3c4043;
            border-radius: 9999px;
            padding: 0 18px;
            margin-bottom: 32px;
            transition: background 0.15s ease, border-color 0.15s ease;
        }
        .search-form:hover {
            background: #35373b;
        }
        .search-form:focus-within {
            background: #303134;
            border-color: #669df6;
        }
        .search-input {
            flex: 1;
            min-width: 0;
            background: transparent;
            border: none;
            color: #e8eaed;
            font-size: 15px;
            outline: none;
        }
        .search-input::placeholder {
            color: #9aa0a6;
        }
        .shortcuts {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(88px, 1fr));
            gap: 8px;
            justify-items: center;
        }
        .tile {
            width: 88px;
            padding: 12px 6px;
            border-radius: 8px;
            text-decoration: none;
            color: #e8eaed;
            display: flex;
            flex-direction: column;
            align-items: center;
            gap: 10px;
            transition: background 0.12s ease;
        }
        .tile:hover {
            background: #303134;
        }
        .tile-icon {
            width: 44px;
            height: 44px;
            border-radius: 50%;
            background: #303134;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 16px;
            font-weight: 600;
            color: #e8eaed;
        }
        .tile:hover .tile-icon {
            background: #3c4043;
        }
        .tile-label {
            font-size: 12px;
            color: #e8eaed;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            max-width: 80px;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1 class="brand">Oxide</h1>
        <form class="search-form" action="https://www.google.com/search" method="GET">
            <input class="search-input" type="text" name="q" placeholder="Search Google or type a URL" autofocus autocomplete="off">
        </form>
        <div class="shortcuts">
            <a class="tile" href="https://www.google.com"><div class="tile-icon">G</div><span class="tile-label">Google</span></a>
            <a class="tile" href="https://www.youtube.com"><div class="tile-icon">Y</div><span class="tile-label">YouTube</span></a>
            <a class="tile" href="https://www.rust-lang.org"><div class="tile-icon">R</div><span class="tile-label">Rust Lang</span></a>
            <a class="tile" href="https://news.ycombinator.com"><div class="tile-icon">H</div><span class="tile-label">Hacker News</span></a>
            <a class="tile" href="https://github.com"><div class="tile-icon">GH</div><span class="tile-label">GitHub</span></a>
            <a class="tile" href="https://en.wikipedia.org"><div class="tile-icon">W</div><span class="tile-label">Wikipedia</span></a>
        </div>
    </div>
</body>
</html>"#
}

/// Returns `true` when `a` and `b` share scheme, host, and effective port (SOP).
#[must_use]
pub fn is_same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// Validates a CORS response or preflight (`OPTIONS`) response against `request_origin` and `method`.
pub fn validate_cors_response(
    request_origin: &str,
    response_headers: &std::collections::HashMap<String, String>,
    method: &str,
) -> Result<(), String> {
    let Some(allow_origin) = response_headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("access-control-allow-origin"))
        .map(|(_, v)| v.trim())
    else {
        return Err("Missing Access-Control-Allow-Origin header".to_string());
    };

    if allow_origin != "*" && !allow_origin.eq_ignore_ascii_case(request_origin) {
        return Err(format!("Origin {request_origin} not allowed by CORS ({allow_origin})"));
    }

    if !matches!(method.to_ascii_uppercase().as_str(), "GET" | "HEAD" | "POST")
        && let Some(allow_methods) = response_headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("access-control-allow-methods"))
            .map(|(_, v)| v)
    {
        let upper = method.to_ascii_uppercase();
        if !allow_methods
            .split(',')
            .any(|m| m.trim().eq_ignore_ascii_case(&upper))
        {
            return Err(format!("Method {upper} not in Access-Control-Allow-Methods"));
        }
    }

    Ok(())
}

/// Parsed Content Security Policy (`default-src`, `script-src`, `style-src`, `img-src`, `connect-src`).
#[derive(Debug, Clone, Default)]
pub struct ContentSecurityPolicy {
    directives: std::collections::HashMap<String, Vec<String>>,
}

impl ContentSecurityPolicy {
    #[must_use]
    pub fn parse(header: &str) -> Self {
        let mut directives = std::collections::HashMap::new();
        for part in header.split(';') {
            let mut tokens = part.split_whitespace();
            let Some(name) = tokens.next() else {
                continue;
            };
            let sources: Vec<String> = tokens.map(|s| s.to_ascii_lowercase()).collect();
            directives.insert(name.to_ascii_lowercase(), sources);
        }
        Self { directives }
    }

    /// Returns `true` if `target_url` is permitted by `directive` (falling back to `default-src`).
    #[must_use]
    pub fn allows(&self, directive: &str, page_url: &Url, target_url: &Url) -> bool {
        let Some(sources) = self
            .directives
            .get(&directive.to_ascii_lowercase())
            .or_else(|| self.directives.get("default-src"))
        else {
            return true;
        };

        if sources.iter().any(|s| s == "'none'") {
            return false;
        }
        for src in sources {
            if src == "*" {
                return true;
            }
            if src == "'self'" && is_same_origin(page_url, target_url) {
                return true;
            }
            if let Some(scheme) = src.strip_suffix(':')
                && target_url.scheme().eq_ignore_ascii_case(scheme)
            {
                return true;
            }
            if let Ok(parsed) = Url::parse(src)
                && is_same_origin(&parsed, target_url)
            {
                return true;
            }
            if let Some(host) = target_url.host_str()
                && src.eq_ignore_ascii_case(host)
            {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_omnibox_google_search() {
        assert_eq!(
            resolve_omnibox_input("rust web browser"),
            "https://www.google.com/search?q=rust%20web%20browser"
        );
        assert_eq!(
            resolve_omnibox_input("weather"),
            "https://www.google.com/search?q=weather"
        );
    }

    #[test]
    fn test_resolve_omnibox_domains_and_schemes() {
        assert_eq!(
            resolve_omnibox_input("google.com"),
            "https://google.com/"
        );
        assert_eq!(
            resolve_omnibox_input("https://news.ycombinator.com"),
            "https://news.ycombinator.com/"
        );
        assert_eq!(
            resolve_omnibox_input("oxide://welcome"),
            "oxide://welcome"
        );
        assert_eq!(
            resolve_omnibox_input("localhost:3000"),
            "http://localhost:3000/"
        );
    }

    #[test]
    fn test_security_policy_blocks_dangerous_schemes() {
        let policy = SecurityPolicy::default();
        assert!(matches!(
            policy.validate_navigation("  JaVaScRiPt:alert(1)"),
            NavigationDecision::Block(_)
        ));
        assert!(matches!(
            policy.validate_navigation("file:///C:/Windows/System32/drivers/etc/hosts"),
            NavigationDecision::Block(_)
        ));
        assert!(matches!(
            policy.validate_navigation("data:text/html,<script>alert(1)</script>"),
            NavigationDecision::Block(_)
        ));
    }

    #[test]
    fn test_security_policy_blocks_ssrf_and_private_ips() {
        let policy = SecurityPolicy::default();
        for target in [
            "https://localhost:8080/admin",
            "https://127.0.0.1/secret",
            "https://169.254.169.254/latest/meta-data/",
            "https://192.168.1.1/",
            "https://10.0.0.1/",
            "https://172.16.0.5/",
            "https://[::1]:3000/",
        ] {
            assert!(
                matches!(policy.validate_navigation(target), NavigationDecision::Block(_)),
                "Expected SSRF block for {target}"
            );
        }
    }

    #[test]
    fn test_security_policy_https_only_upgrade() {
        let policy = SecurityPolicy {
            https_only: true,
            block_private_networks: true,
        };
        assert_eq!(
            policy.validate_navigation("http://example.com/path?a=1"),
            NavigationDecision::UpgradeToHttps("https://example.com/path?a=1".to_string())
        );
    }

    #[test]
    fn test_webview_internal_html_gate() {
        use std::sync::atomic::AtomicBool;
        let policy = SecurityPolicy::default();
        let allow_internal = AtomicBool::new(true);

        // First data:text/html navigation (from internal welcome load_html) is allowed and consumes the flag
        assert!(matches!(
            policy.validate_webview_navigation("data:text/html,<h1>Welcome</h1>", &allow_internal),
            NavigationDecision::Allow(_)
        ));

        // Subsequent untrusted data:text/html navigation from a web page is blocked!
        assert!(matches!(
            policy.validate_webview_navigation("data:text/html,<script>evil()</script>", &allow_internal),
            NavigationDecision::Block(_)
        ));
    }

    #[test]
    fn test_sop_cors_and_csp() {
        let a = Url::parse("https://example.com/app").unwrap();
        let b = Url::parse("https://example.com/api").unwrap();
        let c = Url::parse("https://cdn.other.com/lib.js").unwrap();
        assert!(is_same_origin(&a, &b));
        assert!(!is_same_origin(&a, &c));

        let mut headers = std::collections::HashMap::new();
        headers.insert("Access-Control-Allow-Origin".to_string(), "https://example.com".to_string());
        headers.insert("Access-Control-Allow-Methods".to_string(), "GET, PUT".to_string());
        assert!(validate_cors_response("https://example.com", &headers, "PUT").is_ok());
        assert!(validate_cors_response("https://evil.com", &headers, "GET").is_err());
        assert!(validate_cors_response("https://example.com", &headers, "DELETE").is_err());

        let csp = ContentSecurityPolicy::parse("default-src 'self'; script-src 'self' https://cdn.other.com; img-src *");
        assert!(csp.allows("script-src", &a, &b));
        assert!(csp.allows("script-src", &a, &c));
        assert!(!csp.allows("style-src", &a, &c));
        assert!(csp.allows("img-src", &a, &c));
    }
}
