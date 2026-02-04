//! bun process runner.

use blake3::Hasher;
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::process::Stdio;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::task::JoinHandle;

const BUN_SCRIPT_FILENAME: &str = "bun-svelte-compiler.mjs";
const BUN_SCRIPT_SOURCE: &str = r#"import { createInterface } from 'node:readline';
import { stdin, stdout } from 'node:process';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';

let compile = null;
try {
  const require = createRequire(pathToFileURL(process.cwd() + '/'));
  const compilerPath = require.resolve('svelte/compiler');
  const mod = await import(pathToFileURL(compilerPath).href);
  compile = mod.compile;
} catch (err) {
  const message = err && err.message ? err.message : String(err);
  console.error(`svelte-check-rs bun runner failed to load svelte/compiler: ${message}`);
  process.exit(2);
}

stdout.write(JSON.stringify({ ready: true }) + '\n');

const rl = createInterface({ input: stdin, crlfDelay: Infinity });

for await (const line of rl) {
  if (!line.trim()) continue;

  let req;
  try {
    req = JSON.parse(line);
  } catch (err) {
    const message = err && err.message ? err.message : String(err);
    stdout.write(JSON.stringify({ id: null, error: `invalid json: ${message}` }) + '\n');
    continue;
  }

  const id = req.id;
  const filename = req.filename;
  const source = req.source;
  const options = req.options || {};

  const compileOptions = {
    filename,
    generate: options.generate || 'client',
    dev: options.dev === undefined ? true : options.dev,
    runes: options.runes
  };

  let diagnostics = [];

  try {
    const result = compile(source, compileOptions);
    if (result && Array.isArray(result.warnings)) {
      diagnostics = result.warnings.map((warning) => ({
        code: warning.code || 'warning',
        message: warning.message || '',
        start: warning.start || { line: 1, column: 0 },
        end: warning.end || warning.start || { line: 1, column: 0 },
        severity: 'warning'
      }));
    }
  } catch (err) {
    const start = err && err.start ? err.start : { line: 1, column: 0 };
    const end = err && err.end ? err.end : start;
    const code = err && err.code ? err.code : 'compile_error';
    const message = err && err.message ? err.message : String(err);
    diagnostics = [{
      code,
      message,
      start,
      end,
      severity: 'error'
    }];
  }

  stdout.write(JSON.stringify({ id, diagnostics }) + '\n');
}
"#;

/// Error types for bun runner.
#[derive(Debug, Error)]
pub enum BunError {
    /// Failed to spawn bun process.
    #[error("failed to spawn bun: {0}")]
    SpawnFailed(#[from] std::io::Error),

    /// bun process exited with error.
    #[error("bun exited with code {code}: {stderr}")]
    ProcessFailed { code: i32, stderr: String },

    /// bun binary not found.
    #[error("bun binary not found at: {0}")]
    NotFound(Utf8PathBuf),

    /// Failed to install bun.
    #[error("failed to install bun: {0}")]
    InstallFailed(String),

    /// bun runner protocol error.
    #[error("bun runner protocol error: {0}")]
    ProtocolError(String),

    /// Failed to parse bun response.
    #[error("failed to parse bun response: {0}")]
    ParseError(String),
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct BunCompileOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runes: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BunInput {
    pub filename: Utf8PathBuf,
    pub source: String,
    pub options: BunCompileOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BunDiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy)]
pub struct BunPosition {
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone)]
pub struct BunDiagnostic {
    pub file: Utf8PathBuf,
    pub code: String,
    pub message: String,
    pub severity: BunDiagnosticSeverity,
    pub start: BunPosition,
    pub end: BunPosition,
}

#[derive(Debug, Serialize)]
struct BunRequest {
    id: u64,
    filename: String,
    source: String,
    options: BunCompileOptions,
}

#[derive(Debug, Deserialize)]
struct BunResponse {
    id: Option<u64>,
    diagnostics: Option<Vec<BunJsDiagnostic>>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BunReady {
    ready: bool,
}

#[derive(Debug, Deserialize)]
struct BunJsDiagnostic {
    code: String,
    message: String,
    start: BunJsPosition,
    end: BunJsPosition,
    severity: String,
}

#[derive(Debug, Deserialize)]
struct BunJsPosition {
    line: u32,
    column: u32,
}

/// The bun runner.
#[derive(Debug, Clone)]
pub struct BunRunner {
    bun_path: Utf8PathBuf,
    workspace_root: Utf8PathBuf,
    script_path: Utf8PathBuf,
    worker_count: usize,
}

impl BunRunner {
    /// Creates a new bun runner.
    pub fn new(
        bun_path: Utf8PathBuf,
        workspace_root: Utf8PathBuf,
        worker_count: usize,
    ) -> Result<Self, BunError> {
        let script_path = ensure_script()?;
        let worker_count = worker_count.max(1);
        Ok(Self {
            bun_path,
            workspace_root,
            script_path,
            worker_count,
        })
    }

    /// Attempts to find bun in workspace, PATH, home directory, or cache.
    /// 1. Workspace node_modules/.bin/bun (if workspace_root provided)
    /// 2. PATH
    /// 3. ~/.bun/bin/bun (default install location)
    /// 4. Cache directory
    pub fn find_bun(workspace_root: Option<&Utf8Path>) -> Option<Utf8PathBuf> {
        if let Some(workspace) = workspace_root {
            let bin = workspace.join("node_modules/.bin");
            if let Some(path) = find_bun_in_bin(&bin) {
                return Some(path);
            }
        }

        if let Ok(path) = which::which("bun") {
            if let Ok(utf8_path) = Utf8PathBuf::try_from(path) {
                return Some(utf8_path);
            }
        }

        // Check ~/.bun/bin/bun (default install location from bun.sh/install)
        if let Some(home) = dirs::home_dir() {
            if let Ok(home) = Utf8PathBuf::try_from(home) {
                let bun_home = home.join(".bun/bin");
                if let Some(path) = find_bun_in_bin(&bun_home) {
                    return Some(path);
                }
            }
        }

        if let Some(cache_dir) = Self::get_cache_dir() {
            let bin = cache_dir.join("node_modules/.bin");
            if let Some(path) = find_bun_in_bin(&bin) {
                return Some(path);
            }
        }

        None
    }

    /// Gets the cache directory for svelte-check-rs.
    pub fn get_cache_dir() -> Option<Utf8PathBuf> {
        dirs::cache_dir()
            .and_then(|p| Utf8PathBuf::try_from(p).ok())
            .map(|p| p.join("svelte-check-rs"))
    }

    /// Gets the version of the installed bun binary.
    pub async fn get_bun_version() -> Result<(String, Utf8PathBuf), BunError> {
        let bun_path = Self::find_bun(None).ok_or_else(|| {
            BunError::InstallFailed("bun not found - run with --bun-update to install".into())
        })?;

        let output = Command::new(&bun_path)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(BunError::SpawnFailed)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BunError::ProcessFailed {
                code: output.status.code().unwrap_or(-1),
                stderr: stderr.to_string(),
            });
        }

        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok((version, bun_path))
    }

    /// Updates bun to the specified version or latest if None.
    ///
    /// Installs bun using the official install script from bun.sh.
    pub async fn update_bun(version: Option<&str>) -> Result<Utf8PathBuf, BunError> {
        Self::install_bun_via_script(version).await
    }

    /// Finds bun or installs it if not found.
    ///
    /// Installs bun using the official install script from bun.sh.
    pub async fn ensure_bun(workspace_root: Option<&Utf8Path>) -> Result<Utf8PathBuf, BunError> {
        if let Some(path) = Self::find_bun(workspace_root) {
            return Ok(path);
        }

        Self::install_bun_via_script(None).await
    }

    /// Installs bun using the official install script from bun.sh.
    ///
    /// - Unix: `curl -fsSL https://bun.sh/install | bash`
    /// - Windows: `powershell -c "irm bun.sh/install.ps1 | iex"`
    async fn install_bun_via_script(version: Option<&str>) -> Result<Utf8PathBuf, BunError> {
        let home = dirs::home_dir()
            .ok_or_else(|| BunError::InstallFailed("could not determine home directory".into()))?;
        let home = Utf8PathBuf::try_from(home).map_err(|_| {
            BunError::InstallFailed("home directory path is not valid UTF-8".into())
        })?;

        eprintln!("Installing bun via bun.sh...");

        #[cfg(unix)]
        {
            // Unix: curl -fsSL https://bun.sh/install | bash [-s bun-vX.Y.Z]
            let script = match version {
                Some(v) => {
                    let v = v.strip_prefix('v').unwrap_or(v);
                    format!("curl -fsSL https://bun.sh/install | bash -s bun-v{}", v)
                }
                None => "curl -fsSL https://bun.sh/install | bash".to_string(),
            };

            let output = Command::new("bash")
                .args(["-c", &script])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|e| {
                    BunError::InstallFailed(format!("failed to run install script: {e}"))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(BunError::InstallFailed(format!(
                    "bun install script failed: {stderr}"
                )));
            }
        }

        #[cfg(windows)]
        {
            // Windows: powershell -c "irm bun.sh/install.ps1 | iex"
            // Note: Version selection on Windows requires BUN_VERSION env var
            let mut cmd = Command::new("powershell");
            cmd.args(["-c", "irm bun.sh/install.ps1 | iex"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            if let Some(v) = version {
                let v = v.strip_prefix('v').unwrap_or(v);
                cmd.env("BUN_VERSION", v);
            }

            let output = cmd.output().await.map_err(|e| {
                BunError::InstallFailed(format!("failed to run install script: {e}"))
            })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(BunError::InstallFailed(format!(
                    "bun install script failed: {stderr}"
                )));
            }
        }

        // Bun installs to ~/.bun/bin/bun
        let bun_bin = home.join(".bun/bin");
        if let Some(path) = find_bun_in_bin(&bun_bin) {
            eprintln!("bun installed at {}", path);
            return Ok(path);
        }

        Err(BunError::InstallFailed(
            "bun binary not found after install (expected at ~/.bun/bin/bun)".into(),
        ))
    }

    /// Runs Svelte compiler diagnostics on input files.
    pub async fn check_files(&self, inputs: Vec<BunInput>) -> Result<Vec<BunDiagnostic>, BunError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        let worker_count = self.worker_count.min(inputs.len()).max(1);
        let mut chunks: Vec<Vec<BunInput>> = vec![Vec::new(); worker_count];
        for (idx, input) in inputs.into_iter().enumerate() {
            chunks[idx % worker_count].push(input);
        }

        let mut handles = Vec::new();
        for chunk in chunks.into_iter().filter(|c| !c.is_empty()) {
            let bun_path = self.bun_path.clone();
            let workspace_root = self.workspace_root.clone();
            let script_path = self.script_path.clone();
            handles.push(tokio::spawn(async move {
                let mut worker = BunWorker::spawn(bun_path, workspace_root, script_path).await?;
                worker.check_batch(chunk).await
            }));
        }

        let mut diagnostics = Vec::new();
        for handle in handles {
            let chunk_diags = handle
                .await
                .map_err(|e| BunError::ProtocolError(format!("join error: {e}")))??;
            diagnostics.extend(chunk_diags);
        }

        Ok(diagnostics)
    }
}

fn find_bun_in_bin(bin: &Utf8Path) -> Option<Utf8PathBuf> {
    let candidates: &[&str] = if cfg!(windows) {
        &["bun.exe", "bun.cmd", "bun"]
    } else {
        &["bun"]
    };

    for candidate in candidates.iter() {
        let path = bin.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

fn ensure_script() -> Result<Utf8PathBuf, BunError> {
    let cache_dir = BunRunner::get_cache_dir()
        .ok_or_else(|| BunError::InstallFailed("could not determine cache directory".into()))?;
    fs::create_dir_all(&cache_dir)
        .map_err(|e| BunError::InstallFailed(format!("failed to create cache dir: {e}")))?;

    let script_path = cache_dir.join(BUN_SCRIPT_FILENAME);
    let mut hasher = Hasher::new();
    hasher.update(BUN_SCRIPT_SOURCE.as_bytes());
    let expected_hash = hasher.finalize();

    if let Ok(existing) = fs::read(&script_path) {
        let mut hasher = Hasher::new();
        hasher.update(&existing);
        if hasher.finalize() == expected_hash {
            return Ok(script_path);
        }
    }

    fs::write(&script_path, BUN_SCRIPT_SOURCE)
        .map_err(|e| BunError::InstallFailed(format!("failed to write bun runner script: {e}")))?;

    Ok(script_path)
}

struct BunWorker {
    child: Child,
    stdin: ChildStdin,
    stdout: tokio::io::Lines<BufReader<ChildStdout>>,
    stderr_task: Option<JoinHandle<String>>,
}

impl BunWorker {
    async fn spawn(
        bun_path: Utf8PathBuf,
        workspace_root: Utf8PathBuf,
        script_path: Utf8PathBuf,
    ) -> Result<Self, BunError> {
        let mut child = Command::new(&bun_path)
            .arg(&script_path)
            .current_dir(&workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(BunError::SpawnFailed)?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| BunError::ProtocolError("failed to open bun stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BunError::ProtocolError("failed to open bun stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| BunError::ProtocolError("failed to open bun stderr".to_string()))?;

        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buffer = String::new();
            let _ = reader.read_to_string(&mut buffer).await;
            buffer
        });

        let mut stdout_reader = BufReader::new(stdout).lines();

        let ready_line = stdout_reader
            .next_line()
            .await
            .map_err(|e| BunError::ProtocolError(format!("failed to read bun ready: {e}")))?;

        let Some(ready_line) = ready_line else {
            let stderr = stderr_task.await.unwrap_or_default();
            let status = child.wait().await.map_err(BunError::SpawnFailed)?;
            return Err(BunError::ProcessFailed {
                code: status.code().unwrap_or(-1),
                stderr,
            });
        };

        let ready: BunReady = serde_json::from_str(&ready_line)
            .map_err(|e| BunError::ParseError(format!("invalid ready response: {e}")))?;
        if !ready.ready {
            return Err(BunError::ProtocolError(format!(
                "unexpected bun ready response: {}",
                ready_line
            )));
        }

        Ok(Self {
            child,
            stdin,
            stdout: stdout_reader,
            stderr_task: Some(stderr_task),
        })
    }

    async fn check_batch(&mut self, inputs: Vec<BunInput>) -> Result<Vec<BunDiagnostic>, BunError> {
        let mut pending = HashMap::new();
        let mut next_id = 1u64;

        for input in &inputs {
            let id = next_id;
            next_id += 1;

            let request = BunRequest {
                id,
                filename: input.filename.to_string(),
                source: input.source.clone(),
                options: input.options.clone(),
            };

            let line = serde_json::to_string(&request).map_err(|e| {
                BunError::ProtocolError(format!("failed to serialize request: {e}"))
            })?;
            self.stdin.write_all(line.as_bytes()).await.map_err(|e| {
                BunError::ProtocolError(format!("failed to write to bun stdin: {e}"))
            })?;
            self.stdin
                .write_all(b"\n")
                .await
                .map_err(|e| BunError::ProtocolError(format!("failed to write newline: {e}")))?;

            pending.insert(id, input.filename.clone());
        }

        self.stdin
            .flush()
            .await
            .map_err(|e| BunError::ProtocolError(format!("failed to flush bun stdin: {e}")))?;

        let mut diagnostics = Vec::new();

        while !pending.is_empty() {
            let line = self.stdout.next_line().await.map_err(|e| {
                BunError::ProtocolError(format!("failed to read bun response: {e}"))
            })?;

            let Some(line) = line else {
                let stderr = match self.stderr_task.take() {
                    Some(handle) => handle.await.unwrap_or_default(),
                    None => String::new(),
                };
                let status = self.child.wait().await.map_err(BunError::SpawnFailed)?;
                return Err(BunError::ProcessFailed {
                    code: status.code().unwrap_or(-1),
                    stderr,
                });
            };

            let response: BunResponse = serde_json::from_str(&line)
                .map_err(|e| BunError::ParseError(format!("invalid response: {e} ({line})")))?;

            if let Some(error) = response.error {
                return Err(BunError::ProtocolError(error));
            }

            let id = response
                .id
                .ok_or_else(|| BunError::ProtocolError(format!("missing response id: {line}")))?;

            let file = pending
                .remove(&id)
                .ok_or_else(|| BunError::ProtocolError(format!("unexpected response id {id}")))?;

            if let Some(diags) = response.diagnostics {
                diagnostics.extend(diags.into_iter().map(|diag| BunDiagnostic {
                    file: file.clone(),
                    code: diag.code,
                    message: diag.message,
                    severity: match diag.severity.as_str() {
                        "error" => BunDiagnosticSeverity::Error,
                        _ => BunDiagnosticSeverity::Warning,
                    },
                    start: BunPosition {
                        line: diag.start.line.max(1),
                        column: diag.start.column + 1,
                    },
                    end: BunPosition {
                        line: diag.end.line.max(1),
                        column: diag.end.column + 1,
                    },
                }));
            }
        }

        Ok(diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bun_compile_options_default() {
        let options = BunCompileOptions::default();
        assert!(options.runes.is_none());
    }
}
