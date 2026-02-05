//! tsgo process runner for TypeScript type-checking.
//!
//! This crate interfaces with tsgo (the Go-based TypeScript compiler) to perform
//! type-checking on transformed Svelte files. tsgo is expected to be available in
//! the workspace `node_modules/.bin` and can be resolved via `TsgoRunner::resolve_tsgo`.
//!
//! # Example
//!
//! ```ignore
//! use tsgo_runner::{TsgoRunner, TransformedFiles};
//! use camino::Utf8PathBuf;
//! use std::collections::HashMap;
//!
//! #[tokio::main]
//! async fn main() {
//!     let project_root = Utf8PathBuf::from("/path/to/project");
//!     let tsgo_path = TsgoRunner::resolve_tsgo(&project_root).unwrap();
//!     let runner = TsgoRunner::new(
//!         tsgo_path,
//!         project_root,
//!         None,
//!         HashMap::new(),
//!         true,
//!         true,
//!     );
//!
//!     let files = TransformedFiles::new();
//!     let result = runner.check(&files, false).await.unwrap();
//!     let diagnostics = result.diagnostics;
//!
//!     for diag in diagnostics {
//!         println!("{}: {}", diag.file, diag.message);
//!     }
//! }
//! ```

mod kit;
mod parser;
mod runner;

pub use parser::{DiagnosticSeverity, TsgoDiagnostic, TsgoOutput};
pub use runner::{
    TransformedFile, TransformedFiles, TsgoCacheStats, TsgoCheckOutput, TsgoCheckStats, TsgoError,
    TsgoRunner, TsgoTimingStats,
};
