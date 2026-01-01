//! Configuration loading.

use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;
use std::fs;

/// Svelte project configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)] // Fields will be used when config parsing is complete
pub struct SvelteConfig {
    /// File extensions to process.
    #[serde(default)]
    pub extensions: Vec<String>,

    /// Files/patterns to exclude.
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Preprocessor configuration.
    #[serde(default)]
    pub preprocess: Option<serde_json::Value>,
}

impl SvelteConfig {
    /// Loads configuration from a svelte.config.js file.
    ///
    /// Note: Full JS config parsing would require a JS runtime.
    /// For now, this returns a default config.
    pub fn load(project_root: &Utf8Path) -> Self {
        let config_path = project_root.join("svelte.config.js");

        if config_path.exists() {
            // TODO: Parse svelte.config.js using a JS runtime or static analysis
            // For now, return default
            Self::default()
        } else {
            Self::default()
        }
    }

    /// Returns the default file extensions to process.
    pub fn file_extensions(&self) -> Vec<&str> {
        if self.extensions.is_empty() {
            vec![".svelte"]
        } else {
            self.extensions.iter().map(|s| s.as_str()).collect()
        }
    }
}

/// TypeScript configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Fields will be used when tsgo integration is complete
pub struct TsConfig {
    /// Compiler options.
    #[serde(default)]
    pub compiler_options: CompilerOptions,

    /// Include patterns.
    #[serde(default)]
    pub include: Vec<String>,

    /// Exclude patterns.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// TypeScript compiler options.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Fields will be used when tsgo integration is complete
pub struct CompilerOptions {
    /// Target ECMAScript version.
    pub target: Option<String>,

    /// Module system.
    pub module: Option<String>,

    /// Enable strict mode.
    #[serde(default)]
    pub strict: bool,

    /// Base URL for module resolution.
    pub base_url: Option<String>,

    /// Path mappings.
    #[serde(default)]
    pub paths: std::collections::HashMap<String, Vec<String>>,
}

impl TsConfig {
    /// Loads configuration from a tsconfig.json file.
    pub fn load(path: &Utf8Path) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;

        // Remove comments (simple approach, doesn't handle strings)
        let content = remove_json_comments(&content);

        serde_json::from_str(&content).ok()
    }

    /// Finds and loads tsconfig.json from a project root.
    pub fn find(project_root: &Utf8Path) -> Option<(Utf8PathBuf, Self)> {
        let path = project_root.join("tsconfig.json");
        if path.exists() {
            Self::load(&path).map(|config| (path, config))
        } else {
            None
        }
    }
}

/// Removes single-line and multi-line comments from JSON.
fn remove_json_comments(json: &str) -> String {
    let mut result = String::with_capacity(json.len());
    let mut chars = json.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            result.push(c);
            if c == '"' {
                in_string = false;
            } else if c == '\\' {
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            }
        } else if c == '"' {
            result.push(c);
            in_string = true;
        } else if c == '/' {
            match chars.peek() {
                Some('/') => {
                    // Single-line comment
                    chars.next();
                    while let Some(&next) = chars.peek() {
                        if next == '\n' {
                            break;
                        }
                        chars.next();
                    }
                }
                Some('*') => {
                    // Multi-line comment
                    chars.next();
                    while let Some(next) = chars.next() {
                        if next == '*' && chars.peek() == Some(&'/') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {
                    result.push(c);
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_comments() {
        let json = r#"{
            // This is a comment
            "key": "value" /* inline comment */
        }"#;

        let cleaned = remove_json_comments(json);
        assert!(!cleaned.contains("//"));
        assert!(!cleaned.contains("/*"));
        assert!(cleaned.contains("\"key\""));
    }

    #[test]
    fn test_default_extensions() {
        let config = SvelteConfig::default();
        assert_eq!(config.file_extensions(), vec![".svelte"]);
    }
}
