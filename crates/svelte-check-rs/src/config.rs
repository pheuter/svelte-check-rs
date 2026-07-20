//! Configuration loading.

use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

/// Extensions svelte-check-rs natively understands. Longer suffixes come
/// first so `foo.svelte.ts` is not mistaken for a component.
const NATIVE_EXTENSIONS: &[&str] = &[".svelte.ts", ".svelte.js", ".svelte"];

/// The kind of Svelte file being processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvelteFileKind {
    /// A `.svelte` component file with HTML template, script, and styles.
    Component,
    /// A `.svelte.ts` or `.svelte.js` module file with runes but no template.
    Module,
}

impl SvelteFileKind {
    /// Determines the file kind from a file path.
    pub fn from_path(path: &Utf8Path) -> Option<Self> {
        let file_name = path.file_name()?;
        Self::from_filename(file_name)
    }

    /// Determines the file kind from a filename.
    pub fn from_filename(filename: &str) -> Option<Self> {
        if filename.ends_with(".svelte.ts") || filename.ends_with(".svelte.js") {
            Some(Self::Module)
        } else if filename.ends_with(".svelte") {
            Some(Self::Component)
        } else {
            None
        }
    }

    /// Returns true if this is a module file (`.svelte.ts` or `.svelte.js`).
    #[allow(dead_code)]
    pub fn is_module(&self) -> bool {
        matches!(self, Self::Module)
    }

    /// Returns true if this is a component file (`.svelte`).
    #[allow(dead_code)]
    pub fn is_component(&self) -> bool {
        matches!(self, Self::Component)
    }
}

/// Svelte project configuration.
#[derive(Debug, Clone, Default)]
pub struct SvelteConfig {
    /// File extensions to process.
    pub extensions: Vec<String>,

    /// Files/patterns to exclude.
    #[allow(dead_code)]
    pub exclude: Vec<String>,

    /// SvelteKit configuration.
    pub kit: KitConfig,

    /// Compiler options.
    pub compiler_options: SvelteCompilerOptions,
}

/// SvelteKit-specific configuration.
#[derive(Debug, Clone, Default)]
pub struct KitConfig {
    /// Path aliases (e.g., `$lib` -> `./src/lib`).
    pub alias: HashMap<String, String>,
}

/// Svelte compiler options.
#[derive(Debug, Clone, Default)]
pub struct SvelteCompilerOptions {
    /// Enable runes mode.
    pub runes: Option<bool>,

    /// `compilerOptions.experimental.async` from `svelte.config.js`.
    pub experimental_async: Option<bool>,
}

impl SvelteConfig {
    /// Returns the file extensions to walk during discovery.
    ///
    /// Always includes the natively-supported extensions:
    /// - `.svelte` - Component files
    /// - `.svelte.ts` - TypeScript module files with runes
    /// - `.svelte.js` - JavaScript module files with runes
    ///
    /// Any extra extensions declared in `svelte.config.js` (e.g. `.svx` from
    /// mdsvex) are appended so they are still discovered and reported, but the
    /// orchestrator filters out the unrecognized ones with a user-facing
    /// warning rather than feeding them into the type-checker.
    ///
    /// Order matters: longer suffixes must come before `.svelte` so that
    /// `.svelte.ts` matches before `.svelte`.
    pub fn file_extensions(&self) -> Vec<&str> {
        let mut extensions: Vec<&str> = NATIVE_EXTENSIONS.to_vec();
        for ext in &self.extensions {
            let s = ext.as_str();
            if !extensions.contains(&s) {
                extensions.push(s);
            }
        }
        extensions
    }

    /// Returns extensions declared in `svelte.config.js` that we don't
    /// natively support. Files with these extensions are discovered (so we can
    /// report them) but skipped from the rest of the pipeline.
    pub fn unsupported_extensions(&self) -> Vec<&str> {
        self.extensions
            .iter()
            .map(|s| s.as_str())
            .filter(|s| !NATIVE_EXTENSIONS.contains(s))
            .collect()
    }

    /// Returns whether runes mode is enabled (defaults to true for Svelte 5).
    #[allow(dead_code)]
    pub fn runes_enabled(&self) -> bool {
        self.compiler_options.runes.unwrap_or(true)
    }
}

/// TypeScript configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TsConfig {
    /// Compiler options.
    #[serde(default)]
    pub compiler_options: CompilerOptions,

    /// Include patterns.
    #[serde(default)]
    #[allow(dead_code)]
    pub include: Vec<String>,

    /// Exclude patterns (used to filter out files from checking).
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// TypeScript compiler options.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct CompilerOptions {
    /// Target ECMAScript version.
    pub target: Option<String>,

    /// Module system.
    pub module: Option<String>,

    /// Module resolution strategy.
    pub module_resolution: Option<String>,

    /// Enable strict mode.
    #[serde(default)]
    pub strict: bool,

    /// Base URL for module resolution.
    pub base_url: Option<String>,

    /// Path mappings.
    #[serde(default)]
    pub paths: HashMap<String, Vec<String>>,
}

impl CompilerOptions {
    /// Returns true if the module resolution strategy requires explicit file extensions
    /// for relative imports (NodeNext, Node16).
    pub fn requires_explicit_extensions(&self) -> bool {
        // Check moduleResolution first, then fall back to module
        // (when module is NodeNext/Node16, moduleResolution defaults to the same)
        let resolution = self
            .module_resolution
            .as_deref()
            .or(self.module.as_deref())
            .unwrap_or("");

        matches!(resolution.to_lowercase().as_str(), "nodenext" | "node16")
    }
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

    /// Merges SvelteKit aliases into the paths configuration.
    #[allow(dead_code)]
    pub fn merge_svelte_aliases(&mut self, svelte_config: &SvelteConfig) {
        for (alias, path) in &svelte_config.kit.alias {
            // Convert SvelteKit alias format to TypeScript paths format
            // e.g., "$lib" -> "$lib/*" mapping to ["./src/lib/*"]
            let ts_alias = if alias.ends_with("/*") {
                alias.clone()
            } else {
                format!("{}/*", alias)
            };

            let ts_path = if path.ends_with("/*") {
                path.clone()
            } else {
                format!("{}/*", path)
            };

            self.compiler_options
                .paths
                .entry(ts_alias)
                .or_insert_with(|| vec![ts_path]);
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
        assert_eq!(
            config.file_extensions(),
            vec![".svelte.ts", ".svelte.js", ".svelte"]
        );
    }

    #[test]
    fn test_svelte_file_kind() {
        // Component files
        assert_eq!(
            SvelteFileKind::from_filename("App.svelte"),
            Some(SvelteFileKind::Component)
        );
        assert_eq!(
            SvelteFileKind::from_filename("Counter.svelte"),
            Some(SvelteFileKind::Component)
        );

        // Module files
        assert_eq!(
            SvelteFileKind::from_filename("counter.svelte.ts"),
            Some(SvelteFileKind::Module)
        );
        assert_eq!(
            SvelteFileKind::from_filename("state.svelte.js"),
            Some(SvelteFileKind::Module)
        );

        // Not Svelte files
        assert_eq!(SvelteFileKind::from_filename("app.ts"), None);
        assert_eq!(SvelteFileKind::from_filename("app.js"), None);
        assert_eq!(SvelteFileKind::from_filename("README.md"), None);
    }

    #[test]
    fn test_svelte_file_kind_from_path() {
        use camino::Utf8Path;

        assert_eq!(
            SvelteFileKind::from_path(Utf8Path::new("src/lib/App.svelte")),
            Some(SvelteFileKind::Component)
        );
        assert_eq!(
            SvelteFileKind::from_path(Utf8Path::new("src/lib/counter.svelte.ts")),
            Some(SvelteFileKind::Module)
        );
        assert_eq!(
            SvelteFileKind::from_path(Utf8Path::new("src/lib/utils.ts")),
            None
        );
    }

    #[test]
    fn test_runes_default_enabled() {
        let config = SvelteConfig::default();
        assert!(config.runes_enabled());
    }

    #[test]
    fn test_merge_svelte_aliases() {
        let svelte_config = SvelteConfig {
            kit: KitConfig {
                alias: HashMap::from([
                    ("$lib".to_string(), "./src/lib".to_string()),
                    ("$components".to_string(), "./src/components".to_string()),
                ]),
            },
            ..Default::default()
        };

        let mut ts_config = TsConfig::default();
        ts_config.merge_svelte_aliases(&svelte_config);

        assert!(ts_config.compiler_options.paths.contains_key("$lib/*"));
        assert!(ts_config
            .compiler_options
            .paths
            .contains_key("$components/*"));
    }

    #[test]
    fn test_requires_explicit_extensions() {
        // NodeNext requires explicit extensions
        let opts = CompilerOptions {
            module: Some("NodeNext".to_string()),
            ..Default::default()
        };
        assert!(opts.requires_explicit_extensions());

        // Node16 requires explicit extensions
        let opts = CompilerOptions {
            module: Some("Node16".to_string()),
            ..Default::default()
        };
        assert!(opts.requires_explicit_extensions());

        // Case insensitive
        let opts = CompilerOptions {
            module: Some("nodenext".to_string()),
            ..Default::default()
        };
        assert!(opts.requires_explicit_extensions());

        // moduleResolution takes precedence
        let opts = CompilerOptions {
            module: Some("ESNext".to_string()),
            module_resolution: Some("NodeNext".to_string()),
            ..Default::default()
        };
        assert!(opts.requires_explicit_extensions());

        // Bundler does not require explicit extensions
        let opts = CompilerOptions {
            module: Some("ESNext".to_string()),
            module_resolution: Some("bundler".to_string()),
            ..Default::default()
        };
        assert!(!opts.requires_explicit_extensions());

        // Default does not require explicit extensions
        let opts = CompilerOptions::default();
        assert!(!opts.requires_explicit_extensions());
    }

    #[test]
    fn test_file_extensions_defaults_when_unset() {
        let config = SvelteConfig::default();
        assert_eq!(
            config.file_extensions(),
            vec![".svelte.ts", ".svelte.js", ".svelte"]
        );
        assert!(config.unsupported_extensions().is_empty());
    }

    #[test]
    fn test_file_extensions_merges_with_user_extensions() {
        // Issue #126: when svelte.config.js declares extensions like `.svx`
        // from mdsvex, we still need to discover the natively-supported ones
        // (`.svelte`, `.svelte.ts`, `.svelte.js`) AND the user's extras.
        let config = SvelteConfig {
            extensions: vec![".svelte".to_string(), ".svx".to_string()],
            ..Default::default()
        };
        let extensions = config.file_extensions();
        assert!(extensions.contains(&".svelte"));
        assert!(extensions.contains(&".svelte.ts"));
        assert!(extensions.contains(&".svelte.js"));
        assert!(extensions.contains(&".svx"));
        // `.svelte` listed twice (once natively, once by user) must dedupe.
        assert_eq!(extensions.iter().filter(|e| **e == ".svelte").count(), 1);
    }

    #[test]
    fn test_unsupported_extensions_excludes_natives() {
        let config = SvelteConfig {
            extensions: vec![
                ".svelte".to_string(),
                ".svelte.ts".to_string(),
                ".svx".to_string(),
                ".mdx".to_string(),
            ],
            ..Default::default()
        };
        let mut unsupported = config.unsupported_extensions();
        unsupported.sort();
        assert_eq!(unsupported, vec![".mdx", ".svx"]);
    }
}
