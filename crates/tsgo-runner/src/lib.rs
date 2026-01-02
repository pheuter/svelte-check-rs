//! tsgo process runner for TypeScript type-checking.
//!
//! This crate interfaces with tsgo (the Go-based TypeScript compiler) to perform
//! type-checking on transformed Svelte files.
//!
//! # Example
//!
//! ```ignore
//! use tsgo_runner::{TsgoRunner, TransformedFiles};
//! use camino::Utf8PathBuf;
//!
//! #[tokio::main]
//! async fn main() {
//!     let runner = TsgoRunner::new(
//!         Utf8PathBuf::from("/usr/local/bin/tsgo"),
//!         Utf8PathBuf::from("/path/to/project"),
//!         None,
//!     );
//!
//!     let files = TransformedFiles::new();
//!     let diagnostics = runner.check(&files).await.unwrap();
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
pub use runner::{TransformedFile, TransformedFiles, TsgoError, TsgoRunner};
