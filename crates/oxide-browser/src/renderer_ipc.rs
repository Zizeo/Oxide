//! Multi-process site-isolation IPC protocol (`--renderer` child process).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RendererRequest {
    ParseAndEval { html: String, script: Option<String> },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RendererResponse {
    pub title: Option<String>,
    pub node_count: usize,
    pub console_logs: Vec<String>,
}

/// Executes a single renderer request (used by the `--renderer` child process
/// loop and as an in-process fallback).
pub fn handle_renderer_request(req: RendererRequest) -> Option<RendererResponse> {
    match req {
        RendererRequest::Shutdown => None,
        RendererRequest::ParseAndEval { html, script } => {
            let doc = oxide_dom::Document::parse(&html);
            let title = doc.title();
            let node_count = doc.len();
            let doc_arc = std::sync::Arc::new(std::sync::Mutex::new(doc));
            let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let mutated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let mut js = oxide_js::JsRuntime::new(doc_arc.clone(), logs.clone(), mutated);
            js.execute_document_scripts();
            if let Some(code) = script {
                let _ = js.eval(&code);
            }
            let out_logs = logs.lock().map(|l| l.clone()).unwrap_or_default();
            Some(RendererResponse {
                title,
                node_count,
                console_logs: out_logs,
            })
        }
    }
}

/// Entry point when `oxide-browser` is spawned with `--renderer`.
pub fn run_renderer_process() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(req) = serde_json::from_str::<RendererRequest>(&line) else {
            continue;
        };
        let Some(resp) = handle_renderer_request(req) else {
            break;
        };
        if let Ok(json) = serde_json::to_string(&resp) {
            let _ = writeln!(stdout, "{json}");
            let _ = stdout.flush();
        }
    }
}

/// Tracks isolated child renderer processes keyed by site origin.
#[derive(Default)]
pub struct SiteIsolationManager {
    processes: HashMap<String, Child>,
}

impl SiteIsolationManager {
    pub fn ensure_site_process(&mut self, site: &str) -> bool {
        if self.processes.contains_key(site) {
            return true;
        }
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        let Ok(child) = Command::new(exe)
            .arg("--renderer")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
        else {
            return false;
        };
        self.processes.insert(site.to_string(), child);
        true
    }

    pub fn send_request(
        &mut self,
        site: &str,
        req: &RendererRequest,
    ) -> Option<RendererResponse> {
        let child = self.processes.get_mut(site)?;
        let stdin = child.stdin.as_mut()?;
        let json = serde_json::to_string(req).ok()?;
        writeln!(stdin, "{json}").ok()?;
        stdin.flush().ok()?;
        let stdout = child.stdout.as_mut()?;
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).ok()?;
        serde_json::from_str(&line).ok()
    }

    pub fn shutdown_all(&mut self) {
        for (_, mut child) in self.processes.drain() {
            let _ = child.kill();
        }
    }
}

impl Drop for SiteIsolationManager {
    fn drop(&mut self) {
        self.shutdown_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_ipc_round_trips_html_and_script() {
        let req = RendererRequest::ParseAndEval {
            html: "<html><head><title>Iso</title></head><body><p>Hi</p></body></html>".to_string(),
            script: Some("console.log('isolated');".to_string()),
        };
        let resp = handle_renderer_request(req).unwrap();
        assert_eq!(resp.title.as_deref(), Some("Iso"));
        assert!(resp.node_count > 0);
        assert!(resp.console_logs.iter().any(|l| l.contains("isolated")));
    }
}
