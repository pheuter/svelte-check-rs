//! tsgo process runner.

use crate::kit;
use crate::parser::{parse_tsgo_output, TsgoDiagnostic};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::{Map, Value};
use source_map::SourceMap;
use std::collections::HashMap;
use std::process::Stdio;
use thiserror::Error;
use tokio::process::Command;
use walkdir::WalkDir;

/// Error types for tsgo runner.
#[derive(Debug, Error)]
pub enum TsgoError {
    /// Failed to spawn tsgo process.
    #[error("failed to spawn tsgo: {0}")]
    SpawnFailed(#[from] std::io::Error),

    /// tsgo process exited with error.
    #[error("tsgo exited with code {code}: {stderr}")]
    ProcessFailed { code: i32, stderr: String },

    /// Failed to parse tsgo output.
    #[error("failed to parse tsgo output: {0}")]
    ParseFailed(String),

    /// tsconfig not found.
    #[error("tsconfig not found at: {0}")]
    TsconfigNotFound(Utf8PathBuf),

    /// tsgo binary not found.
    #[error("tsgo binary not found at: {0}")]
    NotFound(Utf8PathBuf),

    /// Failed to write temporary files.
    #[error("failed to write temporary files: {0}")]
    TempFileFailed(String),

    /// Failed to install tsgo.
    #[error("failed to install tsgo: {0}")]
    InstallFailed(String),

    /// npm not found.
    #[error("npm not found - please install Node.js to auto-download tsgo")]
    NpmNotFound,
}

/// The tsgo runner.
pub struct TsgoRunner {
    /// Path to the tsgo binary.
    tsgo_path: Utf8PathBuf,
    /// Project root directory.
    project_root: Utf8PathBuf,
    /// Optional tsconfig path override.
    tsconfig_path: Option<Utf8PathBuf>,
}

impl TsgoRunner {
    /// Creates a new tsgo runner.
    pub fn new(
        tsgo_path: Utf8PathBuf,
        project_root: Utf8PathBuf,
        tsconfig_path: Option<Utf8PathBuf>,
    ) -> Self {
        Self {
            tsgo_path,
            project_root,
            tsconfig_path,
        }
    }

    /// Attempts to find tsgo in PATH or common locations.
    pub fn find_tsgo() -> Option<Utf8PathBuf> {
        // Try PATH first
        if let Ok(path) = which::which("tsgo") {
            if let Ok(utf8_path) = Utf8PathBuf::try_from(path) {
                return Some(utf8_path);
            }
        }

        // Try common locations
        let common_paths = ["/usr/local/bin/tsgo", "/usr/bin/tsgo", "~/.local/bin/tsgo"];

        for path in common_paths {
            let expanded = shellexpand::tilde(path);
            let path = Utf8Path::new(expanded.as_ref());
            if path.exists() {
                return Some(path.to_owned());
            }
        }

        // Try cache directory
        if let Some(cache_dir) = Self::get_cache_dir() {
            let tsgo_path = cache_dir.join("node_modules/.bin/tsgo");
            if tsgo_path.exists() {
                return Some(tsgo_path);
            }
        }

        None
    }

    /// Gets the cache directory for svelte-check-rs.
    fn get_cache_dir() -> Option<Utf8PathBuf> {
        // Use XDG cache dir on Linux, ~/Library/Caches on macOS, etc.
        dirs::cache_dir()
            .and_then(|p| Utf8PathBuf::try_from(p).ok())
            .map(|p| p.join("svelte-check-rs"))
    }

    /// Finds tsgo or installs it if not found.
    ///
    /// This will:
    /// 1. Check if tsgo is in PATH
    /// 2. Check common installation locations
    /// 3. Check the cache directory
    /// 4. If not found, install via npm in the cache directory
    pub async fn ensure_tsgo() -> Result<Utf8PathBuf, TsgoError> {
        // First try to find existing installation
        if let Some(path) = Self::find_tsgo() {
            return Ok(path);
        }

        // Need to install - check if npm is available
        if which::which("npm").is_err() {
            return Err(TsgoError::NpmNotFound);
        }

        // Get or create cache directory
        let cache_dir = Self::get_cache_dir().ok_or_else(|| {
            TsgoError::InstallFailed("could not determine cache directory".into())
        })?;

        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| TsgoError::InstallFailed(format!("failed to create cache dir: {e}")))?;

        eprintln!("tsgo not found, installing @typescript/native-preview...");

        // Run npm install in cache directory
        let output = Command::new("npm")
            .args(["install", "@typescript/native-preview"])
            .current_dir(&cache_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| TsgoError::InstallFailed(format!("failed to run npm: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TsgoError::InstallFailed(format!(
                "npm install failed: {stderr}"
            )));
        }

        // Verify installation
        let tsgo_path = cache_dir.join("node_modules/.bin/tsgo");
        if !tsgo_path.exists() {
            return Err(TsgoError::InstallFailed(
                "tsgo binary not found after npm install".into(),
            ));
        }

        eprintln!("tsgo installed successfully at {}", tsgo_path);
        Ok(tsgo_path)
    }

    /// Resolve the tsconfig path to use.
    fn resolve_tsconfig_path(&self) -> Result<Utf8PathBuf, TsgoError> {
        let candidate = if let Some(path) = &self.tsconfig_path {
            if path.is_relative() {
                self.project_root.join(path)
            } else {
                path.clone()
            }
        } else {
            self.project_root.join("tsconfig.json")
        };

        if candidate.exists() {
            Ok(candidate)
        } else {
            Err(TsgoError::TsconfigNotFound(candidate))
        }
    }

    /// Prepare a tsconfig overlay inside the temp directory.
    ///
    /// This symlinks the existing project tsconfig into the temp workspace so
    /// relative paths (like `./.svelte-kit/tsconfig.json`) resolve against the temp root.
    fn prepare_tsconfig_overlay(
        &self,
        temp_root: &Utf8Path,
        tsconfig_path: &Utf8Path,
        overrides: Option<&Map<String, Value>>,
    ) -> Result<Utf8PathBuf, TsgoError> {
        let temp_tsconfig = if let Ok(rel) = tsconfig_path.strip_prefix(&self.project_root) {
            temp_root.join(rel)
        } else {
            // Fall back to placing the config at the temp root
            let name = tsconfig_path.file_name().unwrap_or("tsconfig.json");
            temp_root.join(name)
        };

        if let Some(parent) = temp_tsconfig.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
        }

        if !temp_tsconfig.exists() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(tsconfig_path, &temp_tsconfig)
                .map_err(|e| TsgoError::TempFileFailed(format!("symlink tsconfig: {e}")))?;

            #[cfg(windows)]
            std::os::windows::fs::symlink_file(tsconfig_path, &temp_tsconfig)
                .map_err(|e| TsgoError::TempFileFailed(format!("symlink tsconfig: {e}")))?;
        }

        if let Some(overrides) = overrides {
            if !overrides.is_empty() {
                let overlay_path = temp_tsconfig.with_file_name("tsconfig.tsgo.json");
                let extends_value = temp_tsconfig
                    .file_name()
                    .unwrap_or("tsconfig.json")
                    .to_string();
                let mut root = Map::new();
                root.insert(
                    "extends".to_string(),
                    Value::String(format!("./{}", extends_value)),
                );
                root.insert(
                    "compilerOptions".to_string(),
                    Value::Object(overrides.clone()),
                );
                let content = serde_json::to_string_pretty(&Value::Object(root))
                    .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                std::fs::write(&overlay_path, content)
                    .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                return Ok(overlay_path);
            }
        }

        Ok(temp_tsconfig)
    }

    /// Mirrors the entire source directory tree from the project to the temp directory.
    /// This preserves the exact directory structure so all relative imports resolve correctly.
    /// .svelte files are skipped since we write transformed .tsx versions.
    fn symlink_source_tree(
        project_root: &Utf8Path,
        project_src: &Utf8Path,
        temp_src: &Utf8Path,
        apply_tsgo_fixes: bool,
    ) -> Result<(), TsgoError> {
        if !project_src.exists() {
            return Ok(());
        }

        for entry in WalkDir::new(project_src).into_iter().filter_map(|e| e.ok()) {
            let path = match Utf8Path::from_path(entry.path()) {
                Some(p) => p,
                None => continue,
            };

            // Calculate relative path
            let relative = match path.strip_prefix(project_src) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let target = temp_src.join(relative);

            // Create directories
            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&target)
                    .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                continue;
            }

            // Skip .svelte files - we write transformed .ts versions
            if path.extension() == Some("svelte") {
                continue;
            }

            if let Some(kind) = kit::kit_file_kind(path, project_root) {
                let source = std::fs::read_to_string(path)
                    .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                let mut transformed =
                    kit::transform_kit_source(kind, path, &source).unwrap_or(source);
                if apply_tsgo_fixes {
                    if let Some(patched) = patch_promise_all_empty_arrays(&transformed) {
                        transformed = patched;
                    }
                }
                std::fs::write(&target, transformed)
                    .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                continue;
            }

            // Skip if target already exists (transformed file takes precedence)
            if target.exists() {
                continue;
            }

            if apply_tsgo_fixes && is_ts_like_file(path) {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if content.contains("Promise.all") {
                        if let Some(patched) = patch_promise_all_empty_arrays(&content) {
                            std::fs::write(&target, patched)
                                .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
                            continue;
                        }
                    }
                }
            }

            // Prefer hard links to keep paths under the temp root. Fall back to copying.
            if let Err(err) = std::fs::hard_link(path, &target) {
                std::fs::copy(path, &target).map_err(|e| {
                    TsgoError::TempFileFailed(format!(
                        "link/copy {}: hard link error {}, copy error {}",
                        relative, err, e
                    ))
                })?;
            }
        }

        Ok(())
    }

    /// Runs type-checking on the transformed files.
    pub async fn check(&self, files: &TransformedFiles) -> Result<Vec<TsgoDiagnostic>, TsgoError> {
        let keep_temp = std::env::var_os("SVELTE_CHECK_RS_KEEP_TEMP").is_some();
        let strict_function_types =
            read_env_bool("SVELTE_CHECK_RS_TSGO_STRICT_FUNCTION_TYPES").unwrap_or(false);
        let apply_tsgo_fixes = !strict_function_types;
        let mut tsconfig_overrides = Map::new();
        tsconfig_overrides.insert(
            "strictFunctionTypes".to_string(),
            Value::Bool(strict_function_types),
        );

        // Verify tsgo exists
        if !self.tsgo_path.exists() {
            return Err(TsgoError::NotFound(self.tsgo_path.clone()));
        }

        // Create temp directory for transformed files inside the project root
        // so module resolution can traverse up to workspace-level node_modules.
        let temp_root = self.project_root.join(".svelte-check-rs");
        std::fs::create_dir_all(&temp_root)
            .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
        let temp_dir = tempfile::Builder::new()
            .prefix("tmp-")
            .tempdir_in(&temp_root)
            .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
        let temp_path = Utf8Path::from_path(temp_dir.path())
            .ok_or_else(|| TsgoError::TempFileFailed("invalid temp path".to_string()))?;

        // Write transformed files
        let mut tsx_files = Vec::new();
        for (virtual_path, file) in &files.files {
            let full_path = temp_path.join(virtual_path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
            }
            std::fs::write(&full_path, &file.tsx_content)
                .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
            if let Some(file_name) = full_path.file_name() {
                let stub_path = full_path.with_extension("d.ts");
                let stub_content = format!(
                    "export * from \"./{}\";\nexport {{ default }} from \"./{}\";\n",
                    file_name, file_name
                );
                std::fs::write(&stub_path, stub_content)
                    .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
            }
            tsx_files.push(full_path.to_string());
        }

        // Symlink key directories/files from project to temp directory for module resolution
        // Note: Don't symlink 'src' since we write transformed files there
        let symlinks = [
            ("node_modules", self.project_root.join("node_modules")),
            (".svelte-kit", self.project_root.join(".svelte-kit")),
            ("tests", self.project_root.join("tests")),
            ("workflows", self.project_root.join("workflows")),
            ("vite.config.js", self.project_root.join("vite.config.js")),
            ("vite.config.ts", self.project_root.join("vite.config.ts")),
        ];

        for (name, source) in symlinks {
            let target = temp_path.join(name);
            if source.exists() && !target.exists() {
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(&source, &target)
                        .map_err(|e| TsgoError::TempFileFailed(format!("symlink {name}: {e}")))?;
                }
                #[cfg(windows)]
                {
                    if source.is_dir() {
                        std::os::windows::fs::symlink_dir(&source, &target).map_err(|e| {
                            TsgoError::TempFileFailed(format!("symlink {name}: {e}"))
                        })?;
                    } else {
                        std::os::windows::fs::symlink_file(&source, &target).map_err(|e| {
                            TsgoError::TempFileFailed(format!("symlink {name}: {e}"))
                        })?;
                    }
                }
            }
        }

        let project_src = self.project_root.join("src");

        // Symlink the entire source tree for proper module resolution
        // This preserves directory structure so relative imports like `./schema` work
        // and SvelteKit route files (+page.ts, etc.) can access ./$types
        let temp_src = temp_path.join("src");
        Self::symlink_source_tree(
            &self.project_root,
            &project_src,
            &temp_src,
            apply_tsgo_fixes,
        )?;

        // Add a local shim for tsgo-only helpers used in patched sources.
        // Placing this under src keeps it within typical tsconfig include globs.
        let shim_path = temp_src.join("__svelte_check_rs_shims.d.ts");
        std::fs::write(
            &shim_path,
            "declare function __svelte_empty_array<T>(value: () => T): Awaited<T>;\n",
        )
        .map_err(|e| TsgoError::TempFileFailed(format!("write shim: {e}")))?;

        // Use the existing tsconfig via symlink overlay
        let project_tsconfig = self.resolve_tsconfig_path()?;
        let temp_tsconfig =
            self.prepare_tsconfig_overlay(temp_path, &project_tsconfig, Some(&tsconfig_overrides))?;

        // Run tsgo on the temp directory
        let output = Command::new(&self.tsgo_path)
            .arg("--project")
            .arg(&temp_tsconfig)
            .current_dir(temp_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        // Parse output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // tsgo may exit with non-zero on type errors, which is expected
        if !output.status.success() && stderr.contains("error") && !stdout.contains(":") {
            return Err(TsgoError::ProcessFailed {
                code: output.status.code().unwrap_or(-1),
                stderr: stderr.to_string(),
            });
        }

        // Parse diagnostics from output
        let diagnostics = parse_tsgo_output(&stdout, files)?;

        if keep_temp {
            let kept_path = temp_dir.keep();
            eprintln!(
                "svelte-check-rs: keeping temp dir at {}",
                kept_path.display()
            );
        }

        Ok(diagnostics)
    }
}

fn read_env_bool(name: &str) -> Option<bool> {
    let value = std::env::var(name).ok()?;
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn is_ts_like_file(path: &Utf8Path) -> bool {
    matches!(
        path.extension(),
        Some("ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs")
    )
}

/// Work around tsgo's inference bug for conditional empty arrays inside Promise.all.
/// This rewrites `? [] :` / `: []` branches to `([] as Awaited<typeof (<other branch>)>)`
/// inside Promise.all calls so the empty array branch inherits the sibling type.
fn patch_promise_all_empty_arrays(input: &str) -> Option<String> {
    if !input.contains("Promise.all") {
        return None;
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Depth {
        paren: i32,
        bracket: i32,
        brace: i32,
    }

    #[derive(Clone, Copy, Debug)]
    struct TernaryPair {
        question: usize,
        colon: usize,
        depth: Depth,
    }

    fn is_ternary_question(bytes: &[u8], i: usize) -> bool {
        if bytes[i] != b'?' {
            return false;
        }
        let prev = if i == 0 { None } else { Some(bytes[i - 1]) };
        let next = bytes.get(i + 1).copied();
        if prev == Some(b'?') || next == Some(b'?') || next == Some(b'.') {
            return false;
        }
        true
    }

    fn scan_ternary_pairs(
        input: &str,
    ) -> (
        Vec<TernaryPair>,
        std::collections::HashMap<usize, usize>,
        std::collections::HashMap<usize, usize>,
    ) {
        let bytes = input.as_bytes();
        let mut pairs = Vec::new();
        let mut question_map = std::collections::HashMap::new();
        let mut colon_map = std::collections::HashMap::new();
        let mut stack: Vec<(usize, Depth)> = Vec::new();
        let mut depth = Depth {
            paren: 0,
            bracket: 0,
            brace: 0,
        };
        let mut i = 0usize;
        let mut in_string: Option<u8> = None;
        let mut in_line_comment = false;
        let mut in_block_comment = false;

        while i < bytes.len() {
            let b = bytes[i];

            if in_line_comment {
                if b == b'\n' {
                    in_line_comment = false;
                }
                i += 1;
                continue;
            }

            if in_block_comment {
                if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    i += 2;
                    in_block_comment = false;
                    continue;
                }
                i += 1;
                continue;
            }

            if let Some(quote) = in_string {
                if b == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if b == quote {
                    in_string = None;
                }
                i += 1;
                continue;
            }

            if b == b'/' && i + 1 < bytes.len() {
                if bytes[i + 1] == b'/' {
                    in_line_comment = true;
                    i += 2;
                    continue;
                }
                if bytes[i + 1] == b'*' {
                    in_block_comment = true;
                    i += 2;
                    continue;
                }
            }

            if b == b'\'' || b == b'"' || b == b'`' {
                in_string = Some(b);
                i += 1;
                continue;
            }

            match b {
                b'(' => depth.paren += 1,
                b')' => depth.paren = depth.paren.saturating_sub(1),
                b'[' => depth.bracket += 1,
                b']' => depth.bracket = depth.bracket.saturating_sub(1),
                b'{' => depth.brace += 1,
                b'}' => depth.brace = depth.brace.saturating_sub(1),
                b'?' if is_ternary_question(bytes, i) => {
                    stack.push((i, depth));
                }
                b':' => {
                    if let Some(&(q_pos, q_depth)) = stack.last() {
                        if depth == q_depth {
                            stack.pop();
                            let pair = TernaryPair {
                                question: q_pos,
                                colon: i,
                                depth,
                            };
                            let idx = pairs.len();
                            pairs.push(pair);
                            question_map.insert(q_pos, idx);
                            colon_map.insert(i, idx);
                        }
                    }
                }
                _ => {}
            }

            i += 1;
        }

        (pairs, question_map, colon_map)
    }

    fn trim_span(input: &str, start: usize, end: usize) -> Option<&str> {
        if start >= end || end > input.len() {
            return None;
        }
        let mut s = start;
        let mut e = end;
        let bytes = input.as_bytes();
        while s < e && bytes[s].is_ascii_whitespace() {
            s += 1;
        }
        while e > s && bytes[e - 1].is_ascii_whitespace() {
            e -= 1;
        }
        if s >= e {
            None
        } else {
            Some(&input[s..e])
        }
    }

    fn scan_false_branch_end(input: &str, start: usize, base_depth: Depth) -> usize {
        let bytes = input.as_bytes();
        let mut i = start;
        let mut depth = base_depth;
        let mut in_string: Option<u8> = None;
        let mut in_line_comment = false;
        let mut in_block_comment = false;
        let mut ternary_depth = 0usize;

        while i < bytes.len() {
            let b = bytes[i];

            if in_line_comment {
                if b == b'\n' {
                    in_line_comment = false;
                }
                i += 1;
                continue;
            }

            if in_block_comment {
                if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    i += 2;
                    in_block_comment = false;
                    continue;
                }
                i += 1;
                continue;
            }

            if let Some(quote) = in_string {
                if b == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if b == quote {
                    in_string = None;
                }
                i += 1;
                continue;
            }

            if b == b'/' && i + 1 < bytes.len() {
                if bytes[i + 1] == b'/' {
                    in_line_comment = true;
                    i += 2;
                    continue;
                }
                if bytes[i + 1] == b'*' {
                    in_block_comment = true;
                    i += 2;
                    continue;
                }
            }

            if b == b'\'' || b == b'"' || b == b'`' {
                in_string = Some(b);
                i += 1;
                continue;
            }

            if b == b'?' && is_ternary_question(bytes, i) {
                ternary_depth += 1;
                i += 1;
                continue;
            }

            if b == b':' && ternary_depth > 0 {
                ternary_depth -= 1;
                i += 1;
                continue;
            }

            if ternary_depth == 0
                && depth == base_depth
                && matches!(b, b',' | b')' | b']' | b'}' | b';')
            {
                break;
            }

            match b {
                b'(' => depth.paren += 1,
                b')' => depth.paren = depth.paren.saturating_sub(1),
                b'[' => depth.bracket += 1,
                b']' => depth.bracket = depth.bracket.saturating_sub(1),
                b'{' => depth.brace += 1,
                b'}' => depth.brace = depth.brace.saturating_sub(1),
                _ => {}
            }

            i += 1;
        }

        i
    }

    let (pairs, question_map, colon_map) = scan_ternary_pairs(input);

    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let mut in_string: Option<u8> = None;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut depth = Depth {
        paren: 0,
        bracket: 0,
        brace: 0,
    };
    struct PromiseAllContext {
        paren_depth: i32,
        array_bracket_depth: Option<i32>,
        base_brace: i32,
    }
    let mut promise_all_stack: Vec<PromiseAllContext> = Vec::new();
    let mut pending_promise_all = false;
    let mut last_non_ws: Option<u8> = None;
    let mut last_non_ws_idx: Option<usize> = None;
    let mut changed = false;

    while i < bytes.len() {
        let b = bytes[i];

        if in_line_comment {
            out.push(b as char);
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }

        if in_block_comment {
            out.push(b as char);
            if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                out.push('/');
                i += 2;
                in_block_comment = false;
                continue;
            }
            i += 1;
            continue;
        }

        if let Some(quote) = in_string {
            out.push(b as char);
            if b == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if b == quote {
                in_string = None;
            }
            i += 1;
            continue;
        }

        if b == b'/' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'/' {
                out.push('/');
                out.push('/');
                i += 2;
                in_line_comment = true;
                continue;
            }
            if bytes[i + 1] == b'*' {
                out.push('/');
                out.push('*');
                i += 2;
                in_block_comment = true;
                continue;
            }
        }

        if b == b'\'' || b == b'"' || b == b'`' {
            out.push(b as char);
            in_string = Some(b);
            i += 1;
            continue;
        }

        if bytes[i..].starts_with(b"Promise.all") {
            let prev = if i == 0 { None } else { Some(bytes[i - 1]) };
            let next = bytes.get(i + "Promise.all".len()).copied();
            let prev_ok = prev.map_or(true, |c| !is_ident_char(c));
            let next_ok = next.map_or(true, |c| !is_ident_char(c));
            if prev_ok && next_ok {
                out.push_str("Promise.all");
                i += "Promise.all".len();
                pending_promise_all = true;
                last_non_ws = Some(b'l');
                last_non_ws_idx = Some(i.saturating_sub(1));
                continue;
            }
        }

        if b == b'(' {
            depth.paren += 1;
            out.push('(');
            if pending_promise_all {
                promise_all_stack.push(PromiseAllContext {
                    paren_depth: depth.paren,
                    array_bracket_depth: None,
                    base_brace: depth.brace,
                });
                pending_promise_all = false;
            }
            last_non_ws = Some(b'(');
            last_non_ws_idx = Some(i);
            i += 1;
            continue;
        }

        if b == b')' {
            out.push(')');
            if promise_all_stack
                .last()
                .is_some_and(|ctx| ctx.paren_depth == depth.paren)
            {
                promise_all_stack.pop();
            }
            depth.paren = depth.paren.saturating_sub(1);
            last_non_ws = Some(b')');
            last_non_ws_idx = Some(i);
            i += 1;
            continue;
        }

        if pending_promise_all && !b.is_ascii_whitespace() {
            pending_promise_all = false;
        }

        if b == b'[' && !promise_all_stack.is_empty() {
            if let Some(ctx) = promise_all_stack.last_mut() {
                if ctx.array_bracket_depth.is_none()
                    && depth.paren == ctx.paren_depth
                    && depth.brace == ctx.base_brace
                {
                    ctx.array_bracket_depth = Some(depth.bracket + 1);
                }
            }
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b']' {
                let can_patch = promise_all_stack.last().is_some_and(|ctx| {
                    ctx.array_bracket_depth == Some(depth.bracket) && depth.brace == ctx.base_brace
                });
                if can_patch && matches!(last_non_ws, Some(b'?' | b':')) {
                    let mut other_expr = None;
                    if let Some(last_idx) = last_non_ws_idx {
                        if last_non_ws == Some(b'?') {
                            if let Some(&pair_idx) = question_map.get(&last_idx) {
                                let pair = pairs[pair_idx];
                                let start = pair.colon + 1;
                                let end = scan_false_branch_end(input, start, pair.depth);
                                if let Some(expr) = trim_span(input, start, end) {
                                    if expr != "[]" {
                                        other_expr = Some(expr.to_string());
                                    }
                                }
                            }
                        } else if let Some(&pair_idx) = colon_map.get(&last_idx) {
                            let pair = pairs[pair_idx];
                            if let Some(expr) = trim_span(input, pair.question + 1, pair.colon) {
                                if expr != "[]" {
                                    other_expr = Some(expr.to_string());
                                }
                            }
                        }
                    }
                    if let Some(expr) = other_expr {
                        out.push_str("__svelte_empty_array(() => (");
                        out.push_str(&expr);
                        out.push_str("))");
                    } else {
                        out.push_str("([] as any[])");
                    }
                    changed = true;
                    i = j + 1;
                    last_non_ws = Some(b')');
                    last_non_ws_idx = Some(j);
                    continue;
                }
            }
        }

        if b == b'[' {
            depth.bracket += 1;
        } else if b == b']' {
            depth.bracket = depth.bracket.saturating_sub(1);
            if let Some(ctx) = promise_all_stack.last_mut() {
                if let Some(array_depth) = ctx.array_bracket_depth {
                    if depth.bracket < array_depth {
                        ctx.array_bracket_depth = None;
                    }
                }
            }
        } else if b == b'{' {
            depth.brace += 1;
        } else if b == b'}' {
            depth.brace = depth.brace.saturating_sub(1);
        }

        out.push(b as char);
        if !b.is_ascii_whitespace() {
            last_non_ws = Some(b);
            last_non_ws_idx = Some(i);
        }
        i += 1;
    }

    if changed {
        Some(out)
    } else {
        None
    }
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// A transformed file ready for type-checking.
#[derive(Debug, Clone)]
pub struct TransformedFile {
    /// The original Svelte file path.
    pub original_path: Utf8PathBuf,
    /// The generated TSX content.
    pub tsx_content: String,
    /// The source map for position mapping.
    pub source_map: SourceMap,
    /// Line index for the original source (for position mapping).
    pub original_line_index: source_map::LineIndex,
}

/// A collection of transformed files.
#[derive(Debug, Clone, Default)]
pub struct TransformedFiles {
    /// Map of virtual path to transformed file.
    pub files: HashMap<Utf8PathBuf, TransformedFile>,
}

impl TransformedFiles {
    /// Creates a new empty collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a transformed file.
    pub fn add(&mut self, virtual_path: Utf8PathBuf, file: TransformedFile) {
        self.files.insert(virtual_path, file);
    }

    /// Gets a transformed file by its virtual path.
    pub fn get(&self, virtual_path: &Utf8Path) -> Option<&TransformedFile> {
        self.files.get(virtual_path)
    }

    /// Finds a file by its original path.
    pub fn find_by_original(&self, original_path: &Utf8Path) -> Option<&TransformedFile> {
        self.files
            .values()
            .find(|f| f.original_path == original_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transformed_files() {
        use source_map::LineIndex;

        let original_source = "<script>let x = 1;</script>";
        let mut files = TransformedFiles::new();
        files.add(
            Utf8PathBuf::from("src/App.svelte.ts"),
            TransformedFile {
                original_path: Utf8PathBuf::from("src/App.svelte"),
                tsx_content: "// generated".to_string(),
                source_map: SourceMap::new(),
                original_line_index: LineIndex::new(original_source),
            },
        );

        assert!(files.get(Utf8Path::new("src/App.svelte.ts")).is_some());
        assert!(files
            .find_by_original(Utf8Path::new("src/App.svelte"))
            .is_some());
    }

    #[test]
    fn test_patch_promise_all_empty_arrays() {
        let input = "const [a] = await Promise.all([cond ? foo() : []]);";
        let patched = patch_promise_all_empty_arrays(input).unwrap();
        assert!(
            patched.contains("cond ? foo() : __svelte_empty_array(() => (foo()))"),
            "patched: {patched}"
        );
    }

    #[test]
    fn test_patch_promise_all_empty_arrays_other_branch() {
        let input = "const [a] = await Promise.all([cond ? [] : foo()]);";
        let patched = patch_promise_all_empty_arrays(input).unwrap();
        assert!(
            patched.contains("cond ? __svelte_empty_array(() => (foo())) : foo()"),
            "patched: {patched}"
        );
    }

    #[test]
    fn test_patch_promise_all_empty_arrays_ignores_other_calls() {
        let input = "const value = cond ? [] : foo();";
        assert!(patch_promise_all_empty_arrays(input).is_none());
    }
}
