//! Configuration loading.

use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use swc_common::SourceMap;
use swc_ecma_ast::{
    ExportDefaultExpr, Expr, KeyValueProp, Lit, ModuleDecl, ModuleItem, ObjectLit, Prop, PropName,
    PropOrSpread,
};
use swc_ecma_parser::{parse_file_as_module, EsSyntax, Syntax, TsSyntax};

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
}

impl SvelteConfig {
    /// Loads configuration from a svelte.config.js file.
    pub fn load(project_root: &Utf8Path) -> Self {
        // Try multiple config file names
        let config_files = ["svelte.config.js", "svelte.config.mjs", "svelte.config.ts"];

        for config_file in config_files {
            let config_path = project_root.join(config_file);
            if config_path.exists() {
                match Self::parse_config(&config_path) {
                    Ok(config) => return config,
                    Err(e) => {
                        eprintln!("Warning: Failed to parse {}: {}", config_path, e);
                        return Self::default();
                    }
                }
            }
        }

        Self::default()
    }

    /// Parses a svelte.config.js or svelte.config.ts file using SWC.
    fn parse_config(path: &Utf8Path) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;

        let cm: Arc<SourceMap> = Default::default();
        let fm = cm.new_source_file(
            swc_common::FileName::Custom(path.to_string()).into(),
            content,
        );

        // Use TypeScript syntax for .ts files, ES for .js/.mjs
        let syntax = if path.as_str().ends_with(".ts") {
            Syntax::Typescript(TsSyntax {
                tsx: false,
                ..Default::default()
            })
        } else {
            Syntax::Es(EsSyntax {
                jsx: false,
                ..Default::default()
            })
        };

        let module = parse_file_as_module(
            &fm,
            syntax,
            swc_ecma_ast::EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .map_err(|e| format!("Parse error: {:?}", e))?;

        let mut config = SvelteConfig::default();

        // Find the default export
        for item in &module.body {
            if let ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(ExportDefaultExpr {
                expr,
                ..
            })) = item
            {
                if let Expr::Object(obj) = expr.as_ref() {
                    Self::extract_config_from_object(obj, &mut config);
                }
            }
        }

        Ok(config)
    }

    /// Gets a string value from a PropName.
    fn prop_name_str(key: &PropName) -> Option<&str> {
        match key {
            PropName::Ident(ident) => Some(ident.sym.as_str()),
            PropName::Str(s) => s.value.as_str(),
            _ => None,
        }
    }

    /// Gets a string value from a Str literal.
    fn str_value(s: &swc_ecma_ast::Str) -> Option<&str> {
        s.value.as_str()
    }

    /// Extracts configuration from an object literal.
    fn extract_config_from_object(obj: &ObjectLit, config: &mut SvelteConfig) {
        for prop in &obj.props {
            if let PropOrSpread::Prop(prop) = prop {
                if let Prop::KeyValue(KeyValueProp { key, value }) = prop.as_ref() {
                    let Some(key_name) = Self::prop_name_str(key) else {
                        continue;
                    };

                    match key_name {
                        "kit" => {
                            if let Expr::Object(kit_obj) = value.as_ref() {
                                Self::extract_kit_config(kit_obj, config);
                            }
                        }
                        "compilerOptions" => {
                            if let Expr::Object(opts_obj) = value.as_ref() {
                                Self::extract_compiler_options(opts_obj, config);
                            }
                        }
                        "extensions" => {
                            if let Expr::Array(arr) = value.as_ref() {
                                for elem in arr.elems.iter().flatten() {
                                    if let Expr::Lit(Lit::Str(s)) = elem.expr.as_ref() {
                                        if let Some(ext) = Self::str_value(s) {
                                            config.extensions.push(ext.to_string());
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Extracts kit configuration.
    fn extract_kit_config(obj: &ObjectLit, config: &mut SvelteConfig) {
        for prop in &obj.props {
            if let PropOrSpread::Prop(prop) = prop {
                if let Prop::KeyValue(KeyValueProp { key, value }) = prop.as_ref() {
                    let Some(key_name) = Self::prop_name_str(key) else {
                        continue;
                    };

                    if key_name == "alias" {
                        if let Expr::Object(alias_obj) = value.as_ref() {
                            Self::extract_aliases(alias_obj, config);
                        }
                    }
                }
            }
        }
    }

    /// Extracts path aliases from the alias object.
    fn extract_aliases(obj: &ObjectLit, config: &mut SvelteConfig) {
        for prop in &obj.props {
            if let PropOrSpread::Prop(prop) = prop {
                if let Prop::KeyValue(KeyValueProp { key, value }) = prop.as_ref() {
                    let Some(alias_name) = Self::prop_name_str(key) else {
                        continue;
                    };

                    if let Expr::Lit(Lit::Str(s)) = value.as_ref() {
                        if let Some(path) = Self::str_value(s) {
                            config
                                .kit
                                .alias
                                .insert(alias_name.to_string(), path.to_string());
                        }
                    }
                }
            }
        }
    }

    /// Extracts compiler options.
    fn extract_compiler_options(obj: &ObjectLit, config: &mut SvelteConfig) {
        for prop in &obj.props {
            if let PropOrSpread::Prop(prop) = prop {
                if let Prop::KeyValue(KeyValueProp { key, value }) = prop.as_ref() {
                    let Some(key_name) = Self::prop_name_str(key) else {
                        continue;
                    };

                    if key_name == "runes" {
                        if let Expr::Lit(Lit::Bool(b)) = value.as_ref() {
                            config.compiler_options.runes = Some(b.value);
                        }
                    }
                }
            }
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

    /// Returns whether runes mode is enabled (defaults to true for Svelte 5).
    #[allow(dead_code)]
    pub fn runes_enabled(&self) -> bool {
        self.compiler_options.runes.unwrap_or(true)
    }
}

/// TypeScript configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
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
#[allow(dead_code)]
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
    pub paths: HashMap<String, Vec<String>>,
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
        assert_eq!(config.file_extensions(), vec![".svelte"]);
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
    fn test_parse_svelte_config_js() {
        // Parse the test fixture svelte.config.js
        let path = Utf8Path::new("../../test-fixtures/projects/simple-app");
        let config = SvelteConfig::load(path);

        // Verify kit.alias was extracted
        assert_eq!(config.kit.alias.get("$lib"), Some(&"./src/lib".to_string()));
        assert_eq!(
            config.kit.alias.get("$components"),
            Some(&"./src/components".to_string())
        );

        // Verify compilerOptions.runes was extracted
        assert_eq!(config.compiler_options.runes, Some(true));

        // Verify extensions were extracted
        assert!(config.extensions.contains(&".svelte".to_string()));
    }

    #[test]
    fn test_parse_svelte_config_inline() {
        // Test parsing inline config
        let config_content = r#"
            export default {
                kit: {
                    alias: {
                        '$lib': './src/lib',
                        '$utils': './src/utils'
                    }
                },
                compilerOptions: {
                    runes: true
                }
            };
        "#;

        // Write to temp file and parse
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("svelte.config.js");
        std::fs::write(&config_path, config_content).unwrap();

        let utf8_path = Utf8PathBuf::try_from(temp_dir).unwrap();
        let config = SvelteConfig::load(&utf8_path);

        assert_eq!(config.kit.alias.get("$lib"), Some(&"./src/lib".to_string()));
        assert_eq!(
            config.kit.alias.get("$utils"),
            Some(&"./src/utils".to_string())
        );
        assert_eq!(config.compiler_options.runes, Some(true));

        // Cleanup
        std::fs::remove_file(config_path).ok();
    }
}
