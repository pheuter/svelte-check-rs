//! tsgo process runner.

use crate::parser::{parse_tsgo_output, TsgoDiagnostic};
use camino::{Utf8Path, Utf8PathBuf};
use source_map::SourceMap;
use std::collections::HashMap;
use std::process::Stdio;
use thiserror::Error;
use tokio::process::Command;

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
}

impl TsgoRunner {
    /// Creates a new tsgo runner.
    pub fn new(tsgo_path: Utf8PathBuf, project_root: Utf8PathBuf) -> Self {
        Self {
            tsgo_path,
            project_root,
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

    /// Runs type-checking on the transformed files.
    pub async fn check(&self, files: &TransformedFiles) -> Result<Vec<TsgoDiagnostic>, TsgoError> {
        // Verify tsgo exists
        if !self.tsgo_path.exists() {
            return Err(TsgoError::NotFound(self.tsgo_path.clone()));
        }

        // Create temp directory for transformed files
        let temp_dir = tempfile::tempdir().map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
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
            tsx_files.push(full_path.to_string());
        }

        // Create a tsconfig.json in temp dir that extends the project's config
        let project_tsconfig = self.project_root.join("tsconfig.json");
        let temp_tsconfig = temp_path.join("tsconfig.json");

        let tsconfig_content = if project_tsconfig.exists() {
            // Extend the project's tsconfig
            format!(
                r#"{{
  "extends": "{}",
  "compilerOptions": {{
    "noEmit": true,
    "skipLibCheck": true
  }},
  "include": ["**/*.tsx"]
}}"#,
                project_tsconfig
            )
        } else {
            // Create a minimal tsconfig
            r#"{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "strict": true,
    "noEmit": true,
    "skipLibCheck": true,
    "jsx": "react-jsx"
  },
  "include": ["**/*.tsx"]
}"#
            .to_string()
        };

        std::fs::write(&temp_tsconfig, &tsconfig_content)
            .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;

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

        Ok(diagnostics)
    }
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
            Utf8PathBuf::from("src/App.svelte.tsx"),
            TransformedFile {
                original_path: Utf8PathBuf::from("src/App.svelte"),
                tsx_content: "// generated".to_string(),
                source_map: SourceMap::new(),
                original_line_index: LineIndex::new(original_source),
            },
        );

        assert!(files.get(Utf8Path::new("src/App.svelte.tsx")).is_some());
        assert!(files
            .find_by_original(Utf8Path::new("src/App.svelte"))
            .is_some());
    }
}
