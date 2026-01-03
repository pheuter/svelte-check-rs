//! Svelte to TypeScript transformation for type-checking.
//!
//! This crate transforms Svelte components into TypeScript code that can be
//! type-checked by `tsgo`. It handles:
//! - Extracting and transforming script content
//! - Converting runes to their TypeScript equivalents
//! - Generating a TypeScript template block for type-checking expressions
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
//! println!("TypeScript output:\n{}", result.tsx_code);
//! ```

mod props;
mod runes;
mod template;
mod transform;
mod types;

pub use props::{extract_props_info, generate_props_type, PropProperty, PropsInfo};
pub use runes::{transform_runes, RuneInfo, RuneKind, RuneMapping, RuneTransformResult};
pub use template::{
    generate_template_check, generate_template_check_with_spans, ExpressionContext,
    GeneratedMapping, TemplateCheckResult, TemplateExpression,
};
pub use transform::{transform, TransformOptions, TransformResult};
pub use types::{component_name_from_path, ComponentExports};
