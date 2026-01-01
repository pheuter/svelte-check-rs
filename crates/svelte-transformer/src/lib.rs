//! Svelte to TSX transformation for type-checking.
//!
//! This crate transforms Svelte components into TypeScript/TSX code that can be
//! type-checked by `tsgo`. It handles:
//! - Extracting and transforming script content
//! - Converting runes to their TypeScript equivalents
//! - Generating a TSX template for type-checking expressions
//! - Building source maps for position mapping
//!
//! # Example
//!
//! ```
//! use svelte_parser::parse;
//! use svelte_transformer::{transform, TransformOptions};
//!
//! let source = r#"
//! <script lang="ts">
//!     let count = $state(0);
//! </script>
//!
//! <button>{count}</button>
//! "#;
//!
//! let parsed = parse(source);
//! let result = transform(&parsed.document, TransformOptions::default());
//! println!("TSX output:\n{}", result.tsx_code);
//! ```

mod runes;
mod template;
mod transform;
mod types;

pub use transform::{transform, TransformOptions, TransformResult};
pub use types::ComponentExports;
