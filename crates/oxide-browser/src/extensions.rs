//! WebExtensions `manifest.json` loader and content-script injector.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContentScriptRule {
    #[serde(default)]
    pub matches: Vec<String>,
    #[serde(default)]
    pub js: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub content_scripts: Vec<ContentScriptRule>,
}

#[derive(Debug, Clone)]
pub struct LoadedExtension {
    pub manifest: ExtensionManifest,
    pub scripts: Vec<(Vec<String>, String)>,
    pub enabled: bool,
}

#[derive(Debug, Default)]
pub struct ExtensionManager {
    pub extensions: Vec<LoadedExtension>,
}

impl ExtensionManager {
    pub fn load_manifest_str(
        &mut self,
        manifest_json: &str,
        script_sources: &[(&str, &str)],
    ) -> Result<(), String> {
        let manifest: ExtensionManifest =
            serde_json::from_str(manifest_json).map_err(|e| e.to_string())?;
        let mut scripts = Vec::new();
        for rule in &manifest.content_scripts {
            for js_path in &rule.js {
                if let Some((_, src)) = script_sources.iter().find(|(p, _)| p == js_path) {
                    scripts.push((rule.matches.clone(), (*src).to_string()));
                }
            }
        }
        self.extensions.push(LoadedExtension {
            manifest,
            scripts,
            enabled: true,
        });
        Ok(())
    }

    pub fn load_from_dir(&mut self, dir: &Path) -> Result<(), String> {
        let manifest_path = dir.join("manifest.json");
        let raw = std::fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
        let manifest: ExtensionManifest =
            serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let mut scripts = Vec::new();
        for rule in &manifest.content_scripts {
            for js_rel in &rule.js {
                if let Ok(code) = std::fs::read_to_string(dir.join(js_rel)) {
                    scripts.push((rule.matches.clone(), code));
                }
            }
        }
        self.extensions.push(LoadedExtension {
            manifest,
            scripts,
            enabled: true,
        });
        Ok(())
    }

    pub fn content_scripts_for_url(&self, url: &str) -> Vec<&str> {
        let mut out = Vec::new();
        for ext in &self.extensions {
            if !ext.enabled {
                continue;
            }
            for (patterns, code) in &ext.scripts {
                if patterns.iter().any(|p| url_matches_pattern(url, p)) {
                    out.push(code.as_str());
                }
            }
        }
        out
    }
}

fn url_matches_pattern(url: &str, pattern: &str) -> bool {
    if pattern == "<all_urls>" || pattern == "*" {
        return url.starts_with("http://") || url.starts_with("https://");
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return url.starts_with(prefix);
    }
    url == pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_manifest_and_matches_content_scripts() {
        let mut mgr = ExtensionManager::default();
        let manifest = r#"{
            "name": "Dark Helper",
            "version": "1.0",
            "permissions": ["tabs"],
            "content_scripts": [{
                "matches": ["https://example.com/*"],
                "js": ["content.js"]
            }]
        }"#;
        mgr.load_manifest_str(manifest, &[("content.js", "console.log('ext');")])
            .unwrap();
        assert_eq!(
            mgr.content_scripts_for_url("https://example.com/page"),
            vec!["console.log('ext');"]
        );
        assert!(mgr.content_scripts_for_url("https://other.com/").is_empty());
    }
}
