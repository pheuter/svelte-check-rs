//! Module file transformation for `.svelte.ts` and `.svelte.js` files.
//!
//! These files are TypeScript/JavaScript modules that can use Svelte runes
//! (`$state`, `$derived`, `$effect`, etc.) but NOT component-specific runes
//! like `$props` or `$bindable`.
//!
//! Unlike `.svelte` component files, module files:
//! - Don't have HTML templates or CSS styles
//! - Don't need the Svelte parser (they're pure TS/JS)
//! - Only need rune transformation
//!
//! # Example
//!
//! ```
//! use svelte_transformer::transform_module;
//!
//! let source = r#"
//! // counter.svelte.ts
//! export function createCounter(initial: number) {
//!     let count = $state(initial);
//!     let doubled = $derived(count * 2);
//!
//!     return {
//!         get count() { return count; },
//!         get doubled() { return doubled; },
//!         increment() { count++; },
//!     };
//! }
//! "#;
//!
//! let result = transform_module(source, None);
//! // Result contains transformed code with runes replaced
//! ```

use crate::runes::{transform_runes, RuneKind, RuneTransformResult};
use smol_str::SmolStr;
use source_map::{SourceMap, SourceMapBuilder};
use std::collections::HashSet;

/// Result of transforming a Svelte module file.
#[derive(Debug)]
pub struct ModuleTransformResult {
    /// The transformed TypeScript/JavaScript code.
    pub code: String,
    /// The source map for position mapping.
    pub source_map: SourceMap,
    /// Store names referenced via $store syntax.
    pub store_names: HashSet<SmolStr>,
    /// Whether the module contains any runes.
    pub has_runes: bool,
    /// Errors encountered during transformation.
    pub errors: Vec<ModuleTransformError>,
}

/// An error that occurred during module transformation.
#[derive(Debug, Clone)]
pub struct ModuleTransformError {
    /// The error message.
    pub message: String,
    /// The line number (1-indexed).
    pub line: usize,
    /// The column number (1-indexed).
    pub column: usize,
}

impl std::fmt::Display for ModuleTransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

/// Transforms a `.svelte.ts` or `.svelte.js` module file.
///
/// This function transforms Svelte runes in module files:
/// - `$state(init)` -> `init`
/// - `$state.raw(init)` -> `init`
/// - `$state.snapshot(x)` -> `(x)`
/// - `$derived(expr)` -> `(expr)`
/// - `$derived.by(fn)` -> `(fn)()`
/// - `$effect(() => {...})` -> `__svelte_effect(() => {...})`
/// - `$effect.pre(() => {...})` -> `__svelte_effect_pre(() => {...})`
/// - `$effect.root(fn)` -> `__svelte_effect_root(fn)`
/// - `$inspect(...)` -> `void 0`
///
/// Component-specific runes (`$props`, `$bindable`, `$host`) are NOT valid in
/// module files and will generate errors if present.
///
/// # Arguments
///
/// * `source` - The source code of the module file
/// * `filename` - Optional filename for error messages
///
/// # Returns
///
/// A `ModuleTransformResult` containing the transformed code, source map,
/// and any errors encountered.
pub fn transform_module(source: &str, filename: Option<&str>) -> ModuleTransformResult {
    let mut builder = SourceMapBuilder::new();
    let mut errors = Vec::new();

    // Add a header comment
    let header = "// Transformed by svelte-check-rs\n";
    builder.add_generated(header);

    // Add helper declarations for effect runes
    // Note: __SvelteLoosen is included to prevent TypeScript errors when users
    // incorrectly use $props in module files (we report a better error message)
    let helpers = r#"// Svelte module rune helpers
declare function __svelte_effect(fn: () => void | (() => void)): void;
declare function __svelte_effect_pre(fn: () => void | (() => void)): void;
declare function __svelte_effect_root(fn: (...args: any[]) => any): void;
type __SvelteLoosen<T> = T extends (...args: any) => any ? T : T extends readonly any[] ? T : T extends object ? T & Record<string, any> : T;

"#;
    builder.add_generated(helpers);

    // Transform runes in the source
    let rune_result: RuneTransformResult = transform_runes(source, 0);

    // Check for component-only runes and report errors
    for rune in &rune_result.runes {
        match rune.kind {
            RuneKind::Props | RuneKind::Bindable | RuneKind::Host => {
                let (line, column) = offset_to_line_column(source, rune.original_span.start.into());
                let rune_name = match rune.kind {
                    RuneKind::Props => "$props",
                    RuneKind::Bindable => "$bindable",
                    RuneKind::Host => "$host",
                    _ => unreachable!(),
                };
                errors.push(ModuleTransformError {
                    message: format!(
                        "{} is only valid inside .svelte component files, not in .svelte.ts/.svelte.js module files{}",
                        rune_name,
                        filename.map(|f| format!(" ({})", f)).unwrap_or_default()
                    ),
                    line,
                    column,
                });
            }
            _ => {}
        }
    }

    // Add the transformed source with proper source mapping using rune mappings
    emit_module_with_rune_mappings(&mut builder, &rune_result.output, &rune_result.mappings);

    let has_runes = !rune_result.runes.is_empty();

    // Build output
    let mut output = String::new();
    output.push_str(header);
    output.push_str(helpers);
    output.push_str(&rune_result.output);

    ModuleTransformResult {
        code: output,
        source_map: builder.build(),
        store_names: rune_result.store_names,
        has_runes,
        errors,
    }
}

/// Emits module content with proper source mappings for rune transformations.
fn emit_module_with_rune_mappings(
    builder: &mut SourceMapBuilder,
    output: &str,
    mappings: &[crate::runes::RuneMapping],
) {
    if mappings.is_empty() {
        // No rune transformations, simple 1:1 mapping
        builder.add_source(0.into(), output);
        return;
    }

    // Sort mappings by generated start position
    let mut sorted_mappings = mappings.to_vec();
    sorted_mappings.sort_by_key(|m| u32::from(m.generated.start));

    let mut gen_pos: usize = 0; // Position in generated output
    let mut orig_pos: u32 = 0; // Position in original

    for mapping in &sorted_mappings {
        let gen_start: usize = u32::from(mapping.generated.start) as usize;
        let gen_end: usize = u32::from(mapping.generated.end) as usize;

        // Emit any unmapped code before this mapping with 1:1 source mapping
        if gen_start > gen_pos {
            let unmapped = &output[gen_pos..gen_start];
            builder.add_source(orig_pos.into(), unmapped);
        }

        // Emit the rune-transformed expression with its original span
        if gen_end <= output.len() {
            let expr = &output[gen_start..gen_end];
            builder.add_transformed(mapping.original, expr);
        }

        gen_pos = gen_end;
        // Update orig_pos to end of the original span
        orig_pos = u32::from(mapping.original.end);
    }

    // Emit any remaining code after the last mapping
    if gen_pos < output.len() {
        let remaining = &output[gen_pos..];
        builder.add_source(orig_pos.into(), remaining);
    }
}

/// Convert a byte offset to line and column numbers (1-indexed).
fn offset_to_line_column(source: &str, offset: u32) -> (usize, usize) {
    let offset = offset as usize;
    let mut line = 1;
    let mut column = 1;
    let mut current_offset = 0;

    for ch in source.chars() {
        if current_offset >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
        current_offset += ch.len_utf8();
    }

    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_state() {
        let source = r#"export function createCounter() {
    let count = $state(0);
    return { get count() { return count; } };
}"#;
        let result = transform_module(source, None);

        assert!(result.code.contains("let count = 0;"));
        assert!(!result.code.contains("$state"));
        assert!(result.has_runes);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_transform_state_with_type() {
        let source = r#"export function createCounter() {
    let count = $state<number>(0);
    return count;
}"#;
        let result = transform_module(source, None);

        assert!(result.code.contains("let count = (0 as number);"));
        assert!(!result.code.contains("$state"));
        assert!(result.has_runes);
    }

    #[test]
    fn test_transform_derived() {
        let source = r#"export function createCounter() {
    let count = $state(0);
    let doubled = $derived(count * 2);
    return { doubled };
}"#;
        let result = transform_module(source, None);

        assert!(result.code.contains("let count = 0;"));
        assert!(result.code.contains("let doubled = (count * 2);"));
        assert!(!result.code.contains("$derived"));
        assert!(result.has_runes);
    }

    #[test]
    fn test_transform_derived_by() {
        let source = r#"export function createComputed() {
    let value = $derived.by(() => expensiveComputation());
    return value;
}"#;
        let result = transform_module(source, None);

        assert!(result
            .code
            .contains("let value = (() => expensiveComputation())();"));
        assert!(!result.code.contains("$derived.by"));
    }

    #[test]
    fn test_transform_effect() {
        let source = r#"export function createLogger() {
    let count = $state(0);
    $effect(() => {
        console.log('Count:', count);
    });
    return { get count() { return count; } };
}"#;
        let result = transform_module(source, None);

        assert!(result.code.contains("__svelte_effect("));
        assert!(!result.code.contains("$effect("));
        assert!(result.has_runes);
    }

    #[test]
    fn test_transform_effect_pre() {
        let source = r#"export function setup() {
    $effect.pre(() => {
        // runs before DOM updates
    });
}"#;
        let result = transform_module(source, None);

        assert!(result.code.contains("__svelte_effect_pre("));
        assert!(!result.code.contains("$effect.pre("));
    }

    #[test]
    fn test_transform_effect_root() {
        let source = r#"export function createRoot() {
    const cleanup = $effect.root(() => {
        // effect that can be cleaned up
        return () => {};
    });
    return cleanup;
}"#;
        let result = transform_module(source, None);

        assert!(result.code.contains("__svelte_effect_root("));
        assert!(!result.code.contains("$effect.root("));
    }

    #[test]
    fn test_transform_state_raw() {
        let source = r#"export function createList() {
    let items = $state.raw([1, 2, 3]);
    return items;
}"#;
        let result = transform_module(source, None);

        assert!(result.code.contains("let items = [1, 2, 3];"));
        assert!(!result.code.contains("$state.raw"));
    }

    #[test]
    fn test_transform_state_snapshot() {
        let source = r#"export function getSnapshot(obj: any) {
    return $state.snapshot(obj);
}"#;
        let result = transform_module(source, None);

        assert!(result.code.contains("return (obj);"));
        assert!(!result.code.contains("$state.snapshot"));
    }

    #[test]
    fn test_transform_inspect() {
        let source = r#"export function debug() {
    let count = $state(0);
    $inspect(count);
    return count;
}"#;
        let result = transform_module(source, None);

        assert!(result.code.contains("void 0;"));
        assert!(!result.code.contains("$inspect"));
    }

    #[test]
    fn test_error_on_props() {
        let source = r#"export function invalid() {
    let { name } = $props();
    return name;
}"#;
        let result = transform_module(source, Some("test.svelte.ts"));

        assert!(!result.errors.is_empty());
        assert!(result.errors[0]
            .message
            .contains("$props is only valid inside .svelte component files"));
    }

    #[test]
    fn test_error_on_bindable() {
        let source = r#"export function invalid() {
    let value = $bindable(0);
    return value;
}"#;
        let result = transform_module(source, None);

        assert!(!result.errors.is_empty());
        assert!(result.errors[0]
            .message
            .contains("$bindable is only valid inside .svelte component files"));
    }

    #[test]
    fn test_error_on_host() {
        let source = r#"export function invalid() {
    $host().dispatchEvent(new Event('test'));
}"#;
        let result = transform_module(source, None);

        assert!(!result.errors.is_empty());
        assert!(result.errors[0]
            .message
            .contains("$host is only valid inside .svelte component files"));
    }

    #[test]
    fn test_no_runes() {
        let source = r#"export function add(a: number, b: number): number {
    return a + b;
}"#;
        let result = transform_module(source, None);

        assert!(!result.has_runes);
        assert!(result.errors.is_empty());
        // The original source should be preserved
        assert!(result.code.contains("return a + b;"));
    }

    #[test]
    fn test_complex_module() {
        let source = r#"// A reusable counter module
export function createCounter(initial: number = 0) {
    let count = $state(initial);
    let doubled = $derived(count * 2);
    let history = $state<number[]>([]);

    $effect(() => {
        history.push(count);
    });

    return {
        get count() { return count; },
        get doubled() { return doubled; },
        get history() { return $state.snapshot(history); },
        increment() { count++; },
        decrement() { count--; },
        reset() { count = initial; },
    };
}"#;
        let result = transform_module(source, None);

        assert!(result.has_runes);
        assert!(result.errors.is_empty());
        assert!(result.code.contains("let count = initial;"));
        assert!(result.code.contains("let doubled = (count * 2);"));
        // $state<number[]>([]) transforms to ([] as number[])
        assert!(result.code.contains("let history = ([] as number[]);"));
        assert!(result.code.contains("__svelte_effect("));
        assert!(result.code.contains("return (history);"));
    }

    #[test]
    fn test_preserves_imports() {
        let source = r#"import { writable } from 'svelte/store';
import type { Readable } from 'svelte/store';

export function createState() {
    let value = $state(0);
    return value;
}"#;
        let result = transform_module(source, None);

        assert!(result
            .code
            .contains("import { writable } from 'svelte/store';"));
        assert!(result
            .code
            .contains("import type { Readable } from 'svelte/store';"));
    }

    #[test]
    fn test_preserves_comments() {
        let source = r#"// This is a comment
/* Block comment */
export function createState() {
    // $state in a comment should not be transformed
    let value = $state(0); // This one should
    return value;
}"#;
        let result = transform_module(source, None);

        // Comments should be preserved
        assert!(result.code.contains("// This is a comment"));
        assert!(result.code.contains("/* Block comment */"));
        // $state in comment should remain
        assert!(result
            .code
            .contains("// $state in a comment should not be transformed"));
        // But the actual $state call should be transformed
        assert!(result.code.contains("let value = 0;"));
    }

    #[test]
    fn test_store_subscriptions() {
        let source = r#"import { myStore } from './stores';

export function updateStore() {
    $myStore = 'new value';
}"#;
        let result = transform_module(source, None);

        // Store names should be collected
        assert!(result.store_names.contains("myStore"));
    }

    #[test]
    fn test_multiple_runes_same_line() {
        let source = r#"export function createPair() {
    let a = $state(1), b = $state(2);
    return { a, b };
}"#;
        let result = transform_module(source, None);

        assert!(result.code.contains("let a = 1, b = 2;"));
        assert!(result.has_runes);
    }

    #[test]
    fn test_nested_function_with_runes() {
        let source = r#"export function outer() {
    let outerCount = $state(0);

    function inner() {
        let innerCount = $state(0);
        return innerCount;
    }

    return { outerCount, inner };
}"#;
        let result = transform_module(source, None);

        assert!(result.code.contains("let outerCount = 0;"));
        assert!(result.code.contains("let innerCount = 0;"));
    }

    #[test]
    fn test_class_with_runes() {
        let source = r#"export class Counter {
    count = $state(0);
    doubled = $derived(this.count * 2);

    increment() {
        this.count++;
    }
}"#;
        let result = transform_module(source, None);

        assert!(result.code.contains("count = 0;"));
        assert!(result.code.contains("doubled = (this.count * 2);"));
        assert!(result.has_runes);
    }
}
