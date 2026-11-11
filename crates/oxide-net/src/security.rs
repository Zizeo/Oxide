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

    // Otherwise, route to Google Search
    let encoded = percent_encoding::utf8_percent_encode(trimmed, percent_encoding::NON_ALPHANUMERIC);
    format!("https://www.google.com/search?q={encoded}")
}

/// Built-in HTML for `oxide://welcome` featuring a Google Search form and quick links.
#[must_use]
pub fn welcome_page_html() -> &'static str {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>New Tab - Oxide Browser</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            background: radial-gradient(circle at top, #1e293b 0%, #0f172a 70%);
            color: #f8fafc;
            min-height: 100vh;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            padding: 24px;
        }
        .container {
            max-width: 720px;
            width: 100%;
            text-align: center;
        }
        .logo {
            font-size: 48px;
            font-weight: 800;
            letter-spacing: -1px;
            background: linear-gradient(135deg, #38bdf8, #818cf8);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            margin-bottom: 8px;
        }
        .subtitle {
            color: #94a3b8;
            font-size: 15px;
            margin-bottom: 32px;
        }
        .search-form {
            display: flex;
            width: 100%;
            background: #1e293b;
            border: 1px solid #334155;
            border-radius: 9999px;
            padding: 6px 8px 6px 20px;
            box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.4);
            margin-bottom: 36px;
            transition: border-color 0.2s;
        }
        .search-form:focus-within {
            border-color: #38bdf8;
        }
        .search-input {
            flex: 1;
            background: transparent;
            border: none;
            color: #f8fafc;
            font-size: 16px;
            outline: none;
        }
        .search-btn {
            background: #38bdf8;
            color: #0f172a;
            border: none;
            border-radius: 9999px;
            padding: 10px 24px;
            font-weight: 600;
            font-size: 14px;
            cursor: pointer;
        }
        .search-btn:hover {
            background: #7dd3fc;
        }
        .grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
            gap: 14px;
            margin-bottom: 32px;
        }
        .card {
            background: rgba(30, 41, 59, 0.75);
            border: 1px solid #334155;
            border-radius: 12px;
            padding: 16px 12px;
            text-decoration: none;
            color: #e2e8f0;
            font-weight: 500;
            font-size: 14px;
            transition: transform 0.15s, border-color 0.15s;
        }
        .card:hover {
            transform: translateY(-2px);
            border-color: #38bdf8;
            color: #38bdf8;
        }
        .shield-banner {
            display: inline-flex;
            align-items: center;
            gap: 8px;
            background: rgba(16, 185, 129, 0.1);
            border: 1px solid rgba(16, 185, 129, 0.3);
            color: #34d399;
            padding: 8px 16px;
            border-radius: 9999px;
            font-size: 13px;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="logo">⚡ Oxide Browser</div>
        <p class="subtitle">Rust Chrome &amp; Security Engine • Powered by Wry WebView</p>
        <form class="search-form" action="https://www.google.com/search" method="GET">
            <input class="search-input" type="text" name="q" placeholder="Search Google or type a query..." autofocus autocomplete="off">
            <button class="search-btn" type="submit">Google Search</button>
        </form>
        <div class="grid">
            <a class="card" href="https://www.google.com">🔍 Google</a>
            <a class="card" href="https://www.youtube.com">▶ YouTube</a>
            <a class="card" href="https://www.rust-lang.org">🦀 Rust Lang</a>
            <a class="card" href="https://news.ycombinator.com">📰 Hacker News</a>
            <a class="card" href="https://github.com">🐙 GitHub</a>
            <a class="card" href="https://en.wikipedia.org">📚 Wikipedia</a>
        </div>
        <div class="shield-banner">
            🛡 Rust Security Guard Active: HTTPS-Only • SSRF &amp; Private LAN Protection • Popup Capture
        </div>
    </div>
</body>
</html>"#
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
}
