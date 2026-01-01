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

        None
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
        for (virtual_path, file) in &files.files {
            let full_path = temp_path.join(virtual_path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
            }
            std::fs::write(&full_path, &file.tsx_content)
                .map_err(|e| TsgoError::TempFileFailed(e.to_string()))?;
        }

        // Find or create tsconfig.json
        let tsconfig_path = self.project_root.join("tsconfig.json");

        // Run tsgo
        let output = Command::new(&self.tsgo_path)
            .arg("--project")
            .arg(&tsconfig_path)
            .arg("--noEmit")
            .current_dir(&self.project_root)
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
        let mut files = TransformedFiles::new();
        files.add(
            Utf8PathBuf::from("src/App.svelte.tsx"),
            TransformedFile {
                original_path: Utf8PathBuf::from("src/App.svelte"),
                tsx_content: "// generated".to_string(),
                source_map: SourceMap::new(),
            },
        );

        assert!(files.get(Utf8Path::new("src/App.svelte.tsx")).is_some());
        assert!(files
            .find_by_original(Utf8Path::new("src/App.svelte"))
            .is_some());
    }
}
