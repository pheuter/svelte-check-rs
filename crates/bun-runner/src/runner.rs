//! bun process runner.

use blake3::Hasher;
use camino::{Utf8Path, Utf8PathBuf};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
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

  if (options.experimental != null && typeof options.experimental === 'object') {
    compileOptions.experimental = options.experimental;
  }

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

/// Subset of Svelte `compile()` options under `experimental`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct BunExperimentalOptions {
    /// `experimental.async` — enables `await` in components when `true`.
    #[serde(rename = "async", skip_serializing_if = "Option::is_none")]
    pub async_: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct BunCompileOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runes: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<BunExperimentalOptions>,
}

#[derive(Debug, Clone)]
pub struct BunInput {
    pub filename: Utf8PathBuf,
    pub source: String,
    pub options: BunCompileOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BunDiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BunPosition {
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    ///
    /// Uses file locking to prevent concurrent installs in monorepo scenarios.
    async fn install_bun_via_script(version: Option<&str>) -> Result<Utf8PathBuf, BunError> {
        let home = dirs::home_dir()
            .ok_or_else(|| BunError::InstallFailed("could not determine home directory".into()))?;
        let home = Utf8PathBuf::try_from(home).map_err(|_| {
            BunError::InstallFailed("home directory path is not valid UTF-8".into())
        })?;

        // Acquire lock to prevent concurrent installs
        let lock_dir = home.join(".bun");
        fs::create_dir_all(&lock_dir)
            .map_err(|e| BunError::InstallFailed(format!("failed to create lock dir: {e}")))?;
        let lock_path = lock_dir.join(".install.lock");
        let lock_file = File::create(&lock_path)
            .map_err(|e| BunError::InstallFailed(format!("failed to create lock file: {e}")))?;
        lock_file
            .lock_exclusive()
            .map_err(|e| BunError::InstallFailed(format!("failed to acquire lock: {e}")))?;

        // Double-check: another process may have installed while we waited for the lock
        let bun_bin = home.join(".bun/bin");
        if let Some(path) = find_bun_in_bin(&bun_bin) {
            return Ok(path);
        }

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

        let svelte_version = self.resolve_svelte_version();
        let cache_dir = self.compiler_cache_dir();
        let mut diagnostics_by_key: HashMap<String, Vec<BunDiagnostic>> = HashMap::new();
        let mut misses = Vec::new();
        let mut ordered_keys = Vec::with_capacity(inputs.len());

        for input in inputs {
            let key = compiler_cache_key(&input, svelte_version.as_deref())?;
            ordered_keys.push(key.clone());
            if let Some(cached) = cache_dir
                .as_ref()
                .and_then(|dir| read_cached_diagnostics(&dir.join(format!("{key}.json"))))
            {
                diagnostics_by_key.insert(key, cached);
            } else {
                misses.push((key, input));
            }
        }

        if !misses.is_empty() {
            let worker_count = self.worker_count.min(misses.len()).max(1);
            let mut chunks: Vec<Vec<(String, BunInput)>> = vec![Vec::new(); worker_count];
            for (idx, entry) in misses.into_iter().enumerate() {
                chunks[idx % worker_count].push(entry);
            }

            let mut handles = Vec::new();
            for chunk in chunks.into_iter().filter(|c| !c.is_empty()) {
                let bun_path = self.bun_path.clone();
                let workspace_root = self.workspace_root.clone();
                let script_path = self.script_path.clone();
                handles.push(tokio::spawn(async move {
                    let mut key_by_file = HashMap::new();
                    let inputs: Vec<BunInput> = chunk
                        .into_iter()
                        .map(|(key, input)| {
                            key_by_file.insert(input.filename.clone(), key);
                            input
                        })
                        .collect();
                    let mut worker =
                        BunWorker::spawn(bun_path, workspace_root, script_path).await?;
                    let diagnostics = worker.check_batch(inputs).await?;
                    Ok::<_, BunError>((key_by_file, diagnostics))
                }));
            }

            for handle in handles {
                let (key_by_file, chunk_diags) = handle
                    .await
                    .map_err(|e| BunError::ProtocolError(format!("join error: {e}")))??;
                for key in key_by_file.values() {
                    diagnostics_by_key.entry(key.clone()).or_default();
                }
                for diag in chunk_diags {
                    if let Some(key) = key_by_file.get(&diag.file) {
                        diagnostics_by_key
                            .entry(key.clone())
                            .or_default()
                            .push(diag);
                    }
                }
            }

            if let Some(dir) = &cache_dir {
                for key in &ordered_keys {
                    if let Some(diagnostics) = diagnostics_by_key.get(key) {
                        let _ =
                            write_cached_diagnostics(&dir.join(format!("{key}.json")), diagnostics);
                    }
                }
            }
        }

        let mut diagnostics = Vec::new();
        for key in ordered_keys {
            if let Some(mut cached) = diagnostics_by_key.remove(&key) {
                diagnostics.append(&mut cached);
            }
        }

        Ok(diagnostics)
    }

    fn compiler_cache_dir(&self) -> Option<Utf8PathBuf> {
        let cache_base = self
            .workspace_node_modules_dir()
            .map(|node_modules| node_modules.join(".cache/svelte-check-rs"))
            .or_else(Self::get_cache_dir)?;
        let cache_dir = cache_base
            .join("compiler-diagnostics")
            .join(project_cache_namespace(&self.workspace_root));
        fs::create_dir_all(&cache_dir).ok()?;
        Some(cache_dir)
    }

    fn workspace_node_modules_dir(&self) -> Option<Utf8PathBuf> {
        let mut current = Some(self.workspace_root.as_path());
        while let Some(dir) = current {
            let candidate = dir.join("node_modules");
            if candidate.is_dir() {
                return Some(candidate);
            }
            current = dir.parent();
        }
        None
    }

    fn resolve_svelte_version(&self) -> Option<String> {
        let mut current = Some(self.workspace_root.as_path());
        while let Some(dir) = current {
            let package_json = dir.join("node_modules/svelte/package.json");
            if let Ok(contents) = fs::read_to_string(&package_json) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) {
                    if let Some(version) = value.get("version").and_then(|v| v.as_str()) {
                        return Some(version.to_string());
                    }
                }
            }
            current = dir.parent();
        }
        None
    }
}

fn compiler_cache_key(input: &BunInput, svelte_version: Option<&str>) -> Result<String, BunError> {
    let options = serde_json::to_vec(&input.options)
        .map_err(|e| BunError::ProtocolError(format!("failed to serialize options: {e}")))?;
    let mut hasher = Hasher::new();
    hasher.update(b"compiler-diagnostics-v1");
    hasher.update(BUN_SCRIPT_SOURCE.as_bytes());
    hasher.update(svelte_version.unwrap_or("unknown").as_bytes());
    hasher.update(input.filename.as_str().as_bytes());
    hasher.update(&[0]);
    hasher.update(input.source.as_bytes());
    hasher.update(&[0]);
    hasher.update(&options);
    Ok(hasher.finalize().to_hex().to_string())
}

fn read_cached_diagnostics(path: &Utf8Path) -> Option<Vec<BunDiagnostic>> {
    let contents = fs::read(path).ok()?;
    serde_json::from_slice(&contents).ok()
}

fn write_cached_diagnostics(
    path: &Utf8Path,
    diagnostics: &[BunDiagnostic],
) -> Result<(), BunError> {
    let contents = serde_json::to_vec(diagnostics)
        .map_err(|e| BunError::ProtocolError(format!("failed to serialize cache: {e}")))?;
    fs::write(path, contents)
        .map_err(|e| BunError::ProtocolError(format!("failed to write cache: {e}")))?;
    Ok(())
}

fn clean_path(path: &Utf8Path) -> Utf8PathBuf {
    let mut prefix: Option<String> = None;
    let mut root: Option<String> = None;
    let mut parts: Vec<String> = Vec::new();

    for component in path.components() {
        match component {
            camino::Utf8Component::Prefix(prefix_component) => {
                prefix = Some(prefix_component.as_str().to_string());
            }
            camino::Utf8Component::RootDir => {
                root = Some(component.as_str().to_string());
            }
            camino::Utf8Component::CurDir => {}
            camino::Utf8Component::ParentDir => {
                if parts.last().is_some_and(|part| part != "..") {
                    parts.pop();
                } else if root.is_none() && prefix.is_none() {
                    parts.push("..".to_string());
                } else {
                    // Already rooted; keep the path normalized at the root.
                }
            }
            camino::Utf8Component::Normal(name) => parts.push(name.to_string()),
        }
    }

    let mut out = Utf8PathBuf::new();
    if let Some(prefix) = prefix {
        out.push(prefix);
    }
    if let Some(root) = root {
        out.push(root);
    }
    for part in parts {
        out.push(part);
    }
    out
}

fn project_cache_namespace(project_root: &Utf8Path) -> String {
    Hasher::new()
        .update(clean_path(project_root).as_str().as_bytes())
        .finalize()
        .to_hex()
        .to_string()
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

        for (id, input) in (1u64..).zip(inputs.iter()) {
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
        assert!(options.experimental.is_none());
    }
}
