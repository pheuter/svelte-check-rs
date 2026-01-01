//! Source position tracking and mapping for svelte-check-rs.
//!
//! This crate provides utilities for tracking source positions through transformations,
//! enabling accurate error reporting that maps generated code positions back to original
//! Svelte source files.

mod builder;
mod line_index;
mod span;

pub use builder::{Mapping, SourceMap, SourceMapBuilder};
pub use line_index::{LineCol, LineIndex};
pub use span::{ByteOffset, Span};
