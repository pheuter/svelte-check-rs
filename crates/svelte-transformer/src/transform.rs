//! Main transformation logic.

use crate::props::{extract_props_info, generate_props_type};
use crate::runes::transform_runes_with_options;
use crate::template::{
    generate_template_check_with_spans, transform_store_subscriptions, TemplateCheckResult,
};
use crate::types::{component_name_from_path, ComponentExports};
use smol_str::SmolStr;
use source_map::{SourceMap, SourceMapBuilder};
use std::collections::HashSet;
use svelte_parser::{
    Attribute, AttributeValue, AttributeValuePart, Fragment, ScriptLang, SvelteDocument,
    TemplateNode,
};

/// The kind of SvelteKit route file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvelteKitRouteKind {
    /// `+page.svelte` - Page component
    Page,
    /// `+layout.svelte` - Layout component
    Layout,
    /// `+error.svelte` - Error component
    Error,
    /// `+page.server.ts/js` - Server-side page load
    PageServer,
    /// `+layout.server.ts/js` - Server-side layout load
    LayoutServer,
    /// `+server.ts/js` - API endpoint
    Server,
    /// Not a SvelteKit route file
    None,
}

impl SvelteKitRouteKind {
    /// Detect the kind of SvelteKit route from a filename.
    ///
    /// Handles SvelteKit's naming conventions including:
    /// - `+page.svelte`, `+layout.svelte`, `+error.svelte`
    /// - `+page@.svelte`, `+layout@.svelte` (breaking out of layouts)
    /// - `+page@group.svelte` (named layout resets)
    pub fn from_filename(filename: &str) -> Self {
        let basename = std::path::Path::new(filename)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(filename);

        // Handle base names and variants with @ suffix (layout resets)
        // e.g., +page.svelte, +page@.svelte, +page@group.svelte
        if basename.starts_with("+page") && basename.ends_with(".svelte") {
            return Self::Page;
        }
        if basename.starts_with("+layout") && basename.ends_with(".svelte") {
            return Self::Layout;
        }
        if basename.starts_with("+error") && basename.ends_with(".svelte") {
            return Self::Error;
        }
        if basename.starts_with("+page.server.") {
            return Self::PageServer;
        }
        if basename.starts_with("+layout.server.") {
            return Self::LayoutServer;
        }
        if basename.starts_with("+server.") {
            return Self::Server;
        }

        Self::None
    }

    /// Get the props type name for this route kind.
    pub fn props_type(&self) -> Option<&'static str> {
        match self {
            Self::Page | Self::PageServer => Some("PageProps"),
            Self::Layout | Self::LayoutServer => Some("LayoutProps"),
            _ => None,
        }
    }
}

/// Options for transformation.
#[derive(Debug, Clone, Default)]
pub struct TransformOptions {
    /// The filename of the source file.
    pub filename: Option<String>,
    /// Whether to generate source maps.
    pub source_maps: bool,
}

/// The result of transformation.
#[derive(Debug)]
pub struct TransformResult {
    /// The generated TSX code.
    pub tsx_code: String,
    /// The source map for position mapping.
    pub source_map: SourceMap,
    /// Exported component types.
    pub exports: ComponentExports,
}

#[derive(Debug, Clone)]
struct SnippetDecl {
    name: String,
    parameters: String,
}

fn collect_top_level_snippets(fragment: &Fragment) -> Vec<SnippetDecl> {
    let mut seen = HashSet::new();
    let mut snippets = Vec::new();

    for node in &fragment.nodes {
        if let TemplateNode::SnippetBlock(block) = node {
            let name = block.name.to_string();
            if seen.insert(name.clone()) {
                snippets.push(SnippetDecl {
                    name,
                    parameters: block.parameters.clone(),
                });
            }
        }
    }

    snippets
}

fn script_indent(script: &str) -> String {
    for line in script.lines() {
        let trimmed = line.trim_start();
        if !trimmed.is_empty() {
            let prefix_len = line.len() - trimmed.len();
            return line[..prefix_len].to_string();
        }
    }
    String::new()
}

fn render_store_aliases(store_names: &HashSet<SmolStr>, indent: &str) -> Option<String> {
    if store_names.is_empty() {
        return None;
    }

    let mut stores: Vec<_> = store_names.iter().collect();
    stores.sort();

    let mut out = String::new();
    for store in stores {
        out.push_str(indent);
        out.push_str("declare let $");
        out.push_str(store);
        out.push_str(": __StoreValue<typeof ");
        out.push_str(store);
        out.push_str(">;\n");
    }
    Some(out)
}

fn collect_declared_types_from_script(script: &str, out: &mut HashSet<String>) {
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut string_delim: Option<char> = None;
    let mut chars = script.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            continue;
        }

        if in_block_comment {
            if ch == '*' && matches!(chars.peek(), Some('/')) {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }

        if let Some(delim) = string_delim {
            if ch == '\\' {
                chars.next();
                continue;
            }
            if ch == delim {
                string_delim = None;
            }
            continue;
        }

        if ch == '/' {
            match chars.peek() {
                Some('/') => {
                    chars.next();
                    in_line_comment = true;
                    continue;
                }
                Some('*') => {
                    chars.next();
                    in_block_comment = true;
                    continue;
                }
                _ => {}
            }
        }

        if matches!(ch, '"' | '\'' | '`') {
            string_delim = Some(ch);
            continue;
        }

        if ch == '_' || ch == '$' || ch.is_ascii_alphabetic() {
            let mut ident = String::new();
            ident.push(ch);
            while let Some(&next) = chars.peek() {
                if next == '_' || next == '$' || next.is_ascii_alphanumeric() {
                    ident.push(next);
                    chars.next();
                } else {
                    break;
                }
            }

            if ident == "type" || ident == "interface" {
                while let Some(&next) = chars.peek() {
                    if next.is_whitespace() {
                        chars.next();
                    } else {
                        break;
                    }
                }

                if let Some(&next) = chars.peek() {
                    if next == '_' || next == '$' || next.is_ascii_alphabetic() {
                        let mut name = String::new();
                        name.push(next);
                        chars.next();
                        while let Some(&c) = chars.peek() {
                            if c == '_' || c == '$' || c.is_ascii_alphanumeric() {
                                name.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        out.insert(name);
                    }
                }
            }
        }
    }
}

fn placeholder_alias_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("type ") {
        return None;
    }

    let rest = trimmed[5..].trim_start();
    let name_end = rest
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(rest.len());
    let name = rest[..name_end].trim();
    if name.is_empty() {
        return None;
    }

    let rest = rest[name_end..].trim_start();
    if !rest.starts_with('=') {
        return None;
    }

    let rhs = rest[1..].trim_start();
    let rhs = rhs.split(';').next().unwrap_or(rhs).trim();
    if rhs == "unknown" || rhs == "any" {
        Some(name.to_string())
    } else {
        None
    }
}

fn collect_placeholder_types(script: &str, out: &mut HashSet<String>) {
    for line in script.lines() {
        if let Some(name) = placeholder_alias_name(line) {
            out.insert(name);
        }
    }
}

fn strip_placeholder_type_aliases(script: &str, placeholders: &HashSet<String>) -> String {
    if placeholders.is_empty() {
        return script.to_string();
    }

    let mut out = String::new();
    for line in script.lines() {
        if let Some(name) = placeholder_alias_name(line) {
            if placeholders.contains(&name) {
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn should_loosen_props_annotation(type_ann: &str) -> bool {
    let trimmed = type_ann.trim();
    if trimmed.starts_with('{') {
        return false;
    }
    trimmed.len() > 120
        || trimmed.contains('.')
        || trimmed.contains('<')
        || trimmed.contains('>')
        || trimmed.contains('&')
        || trimmed.contains('|')
}

fn loosen_props_annotation(script: &str, type_ann: &str) -> String {
    if type_ann.trim().is_empty() {
        return script.to_string();
    }

    if let Some(pos) = script.find(type_ann) {
        let before = script[..pos].chars().rev().find(|c| !c.is_whitespace());
        let after = script[pos + type_ann.len()..]
            .chars()
            .find(|c| !c.is_whitespace());

        if before == Some(':') && after == Some('=') {
            let mut out = String::with_capacity(script.len());
            out.push_str(&script[..pos]);
            out.push_str("any");
            out.push_str(&script[pos + type_ann.len()..]);
            return out;
        }
    }

    script.to_string()
}

fn extract_script_generics(doc: &SvelteDocument) -> Option<String> {
    let script = doc.instance_script.as_ref()?;
    script.attributes.iter().find_map(|attr| match attr {
        Attribute::Normal(normal) if normal.name == "generics" => {
            extract_attribute_text(&normal.value)
        }
        _ => None,
    })
}

fn extract_attribute_text(value: &AttributeValue) -> Option<String> {
    match value {
        AttributeValue::Text(t) => Some(t.value.clone()),
        AttributeValue::Expression(e) => Some(e.expression.clone()),
        AttributeValue::Concat(parts) => {
            let mut combined = String::new();
            for part in parts {
                if let AttributeValuePart::Text(t) = part {
                    combined.push_str(&t.value);
                }
            }
            if combined.is_empty() {
                None
            } else {
                Some(combined)
            }
        }
        AttributeValue::True => None,
    }
}

#[derive(Debug, Clone)]
struct GenericParam {
    name: String,
    definition: String,
}

fn parse_generic_declarations(generics: &str) -> Vec<GenericParam> {
    let params = split_generics(generics);
    params
        .into_iter()
        .filter_map(|param| {
            let param = param.trim();
            if param.is_empty() {
                return None;
            }

            // Extract name up to first whitespace or '='
            let name_end = param
                .find(|c: char| c.is_whitespace() || c == '=')
                .unwrap_or(param.len());
            let name = param[..name_end].trim().to_string();
            if name.is_empty() {
                return None;
            }

            Some(GenericParam {
                name,
                definition: param.to_string(),
            })
        })
        .collect()
}

fn split_generics(generics: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (i, ch) in generics.char_indices() {
        match ch {
            '<' | '(' | '[' | '{' => depth += 1,
            '>' | ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
            }
            ',' if depth == 0 => {
                parts.push(generics[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }

    if start < generics.len() {
        parts.push(generics[start..].to_string());
    }

    parts
}

fn generics_def(params: &[GenericParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let defs = params
        .iter()
        .map(|param| param.definition.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!("<{}>", defs)
}

fn generics_ref(params: &[GenericParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let refs = params
        .iter()
        .map(|param| param.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!("<{}>", refs)
}

fn extract_top_level_imports(script: &str) -> (String, String) {
    let mut imports = String::new();
    let mut output = String::with_capacity(script.len());
    let mut i = 0usize;
    let mut last_emit = 0usize;

    let mut depth = 0usize;
    let mut in_string: Option<char> = None;
    let mut prev_was_escape = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while i < script.len() {
        let ch = script[i..].chars().next().unwrap();
        let ch_len = ch.len_utf8();

        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            i += ch_len;
            continue;
        }

        if in_block_comment {
            if ch == '*' && script[i + ch_len..].starts_with('/') {
                in_block_comment = false;
                i += ch_len + 1;
                continue;
            }
            i += ch_len;
            continue;
        }

        if let Some(quote) = in_string {
            if prev_was_escape {
                prev_was_escape = false;
            } else if ch == '\\' {
                prev_was_escape = true;
            } else if ch == quote {
                in_string = None;
            }
            i += ch_len;
            continue;
        }

        if ch == '/' {
            if script[i + ch_len..].starts_with('/') {
                in_line_comment = true;
                i += ch_len + 1;
                continue;
            } else if script[i + ch_len..].starts_with('*') {
                in_block_comment = true;
                i += ch_len + 1;
                continue;
            }
        }

        if matches!(ch, '\'' | '"' | '`') {
            in_string = Some(ch);
            i += ch_len;
            continue;
        }

        if depth == 0 && script[i..].starts_with("import") {
            let prev_char = script[..i].chars().last();
            let prev_ok = prev_char.map_or(true, |c| c.is_whitespace() || c == ';');
            let next_char = script[i + "import".len()..].chars().next();
            let next_ok = next_char.is_some_and(|c| c.is_whitespace());

            if prev_ok && next_ok {
                let end = read_import_statement(script, i);
                let stmt = &script[i..end];
                output.push_str(&script[last_emit..i]);
                imports.push_str(stmt);

                let newline_count = stmt.chars().filter(|c| *c == '\n').count();
                if newline_count > 0 {
                    output.push_str(&"\n".repeat(newline_count));
                }

                last_emit = end;
                i = end;
                continue;
            }
        }

        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }

        i += ch_len;
    }

    output.push_str(&script[last_emit..]);
    (imports, output)
}

fn read_import_statement(script: &str, start: usize) -> usize {
    let mut i = start;
    let mut depth = 0usize;
    let mut in_string: Option<char> = None;
    let mut prev_was_escape = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut saw_string = false;

    while i < script.len() {
        let ch = script[i..].chars().next().unwrap();
        let ch_len = ch.len_utf8();

        if in_line_comment {
            if ch == '\n' {
                i += ch_len;
                break;
            }
            i += ch_len;
            continue;
        }

        if in_block_comment {
            if ch == '*' && script[i + ch_len..].starts_with('/') {
                in_block_comment = false;
                i += ch_len + 1;
                continue;
            }
            i += ch_len;
            continue;
        }

        if let Some(quote) = in_string {
            if prev_was_escape {
                prev_was_escape = false;
            } else if ch == '\\' {
                prev_was_escape = true;
            } else if ch == quote {
                in_string = None;
            }
            i += ch_len;
            continue;
        }

        if ch == '/' {
            if script[i + ch_len..].starts_with('/') {
                in_line_comment = true;
                i += ch_len + 1;
                continue;
            } else if script[i + ch_len..].starts_with('*') {
                in_block_comment = true;
                i += ch_len + 1;
                continue;
            }
        }

        if matches!(ch, '\'' | '"' | '`') {
            in_string = Some(ch);
            saw_string = true;
            i += ch_len;
            continue;
        }

        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
            }
            ';' if depth == 0 => {
                i += ch_len;
                break;
            }
            '\n' if depth == 0 && saw_string => {
                i += ch_len;
                break;
            }
            _ => {}
        }

        i += ch_len;
    }

    i
}

/// Emits template code with proper source mappings for expressions.
///
/// This function iterates through the template code, emitting unmapped sections
/// with `add_generated()` and mapped sections (expressions) with `add_transformed()`.
fn emit_template_with_mappings(builder: &mut SourceMapBuilder, result: &TemplateCheckResult) {
    if result.mappings.is_empty() {
        // No mappings, just emit as generated
        builder.add_generated(&result.code);
        return;
    }

    // Sort mappings by generated start position
    let mut mappings = result.mappings.clone();
    mappings.sort_by_key(|m| m.generated_start);

    let code = &result.code;
    let mut pos = 0;

    for mapping in &mappings {
        // Emit any unmapped code before this mapping
        if mapping.generated_start > pos {
            let unmapped = &code[pos..mapping.generated_start];
            builder.add_generated(unmapped);
        }

        // Emit the mapped expression
        if mapping.generated_end <= code.len() {
            let expr = &code[mapping.generated_start..mapping.generated_end];
            builder.add_transformed(mapping.original_span, expr);
        }

        pos = mapping.generated_end;
    }

    // Emit any remaining code after the last mapping
    if pos < code.len() {
        let remaining = &code[pos..];
        builder.add_generated(remaining);
    }
}

/// Transforms a Svelte document to TSX.
pub fn transform(doc: &SvelteDocument, options: TransformOptions) -> TransformResult {
    let mut output = String::new();
    let mut builder = SourceMapBuilder::new();
    let mut exports = ComponentExports::default();

    // Detect SvelteKit route kind for type inference
    let route_kind = options
        .filename
        .as_deref()
        .map(SvelteKitRouteKind::from_filename)
        .unwrap_or(SvelteKitRouteKind::None);

    // Add file header
    let header = "// Generated by svelte-check-rs\n// This file is for type-checking only\n\n";
    output.push_str(header);
    builder.add_generated(header);

    // Collect top-level snippets (used for module exports)
    let snippet_decls = collect_top_level_snippets(&doc.fragment);
    let has_snippets = !snippet_decls.is_empty();

    // Extract script generics (e.g., <script generics="T extends ...">)
    let mut generic_decls = extract_script_generics(doc)
        .map(|g| parse_generic_declarations(&g))
        .unwrap_or_default();

    let mut placeholder_types = HashSet::new();
    if let Some(module) = &doc.module_script {
        collect_placeholder_types(&module.content, &mut placeholder_types);
    }
    if let Some(instance) = &doc.instance_script {
        collect_placeholder_types(&instance.content, &mut placeholder_types);
    }

    if !placeholder_types.is_empty() {
        let generic_names: HashSet<_> = generic_decls
            .iter()
            .map(|param| param.name.clone())
            .collect();
        placeholder_types.retain(|name| generic_names.contains(name));
    }

    let mut declared_types = HashSet::new();
    if let Some(module) = &doc.module_script {
        collect_declared_types_from_script(&module.content, &mut declared_types);
    }
    if let Some(instance) = &doc.instance_script {
        collect_declared_types_from_script(&instance.content, &mut declared_types);
    }

    for name in &placeholder_types {
        declared_types.remove(name);
    }

    generic_decls.retain(|param| !declared_types.contains(&param.name));
    let has_generics = !generic_decls.is_empty();
    let generics_def_str = generics_def(&generic_decls);
    let generics_ref_str = generics_ref(&generic_decls);

    // Add Svelte imports - alias to avoid collisions with user imports
    let imports = String::from(
        "import type { ComponentInternals as __SvelteComponentInternals, Snippet as __SvelteSnippet } from 'svelte';\n\
import type { SvelteHTMLElements as __SvelteHTMLElements, HTMLAttributes as __SvelteHTMLAttributes } from 'svelte/elements';\n",
    );
    output.push_str(&imports);
    builder.add_generated(&imports);

    // Add SvelteKit type imports for route files
    if let Some(props_type) = route_kind.props_type() {
        let sveltekit_import = format!("import type {{ {} }} from './$types';\n\n", props_type);
        output.push_str(&sveltekit_import);
        builder.add_generated(&sveltekit_import);
    } else {
        output.push('\n');
        builder.add_generated("\n");
    }

    // Add helper functions for template type-checking
    let helpers = r#"// Helper functions for template type-checking
type __SvelteComponent<
  Props extends Record<string, any> = {},
  Exports extends Record<string, any> = {}
> = {
  (this: void, internals: __SvelteComponentInternals, props: Props): {
    $on?(type: string, callback: (e: any) => void): () => void;
    $set?(props: Partial<Props>): void;
  } & Exports;
  element?: typeof HTMLElement;
  z_$$bindings?: string;
};

declare function __svelte_each_indexed<T>(arr: ArrayLike<T> | Iterable<T>): [number, T][];
declare function __svelte_is_empty<T>(arr: ArrayLike<T> | Iterable<T>): boolean;

// Helper to get store value type from store subscription ($store syntax)
declare function __svelte_store_get<T>(store: { subscribe(fn: (value: T) => void): any }): T;

// Helpers for $effect runes (avoid control-flow narrowing during type-checking)
declare function __svelte_effect(fn: () => void | (() => void)): void;
declare function __svelte_effect_pre(fn: () => void | (() => void)): void;
declare function __svelte_effect_root(fn: (...args: any[]) => any): void;

// Helper type to extract store value for typeof expressions
type __StoreValue<S> = S extends { subscribe(fn: (value: infer T) => void): any } ? T : never;

// Helper to mark specific props as optional without expanding complex unions.
type __SvelteOptionalProps<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

// Loosen props to allow extra top-level fields while preserving declared shapes.
type __SvelteLoosen<T> =
  T extends (...args: any) => any ? T :
  T extends readonly any[] ? T :
  T extends object ? T & Record<string, any> : T;

// Helper for $props.<name>() accessors.
type __SveltePropsAccessor<T> = { [K in keyof T]: () => T[K] } & Record<string, () => any>;

// Shared snippet return value to satisfy Snippet return types.
declare const __svelte_snippet_return: ReturnType<__SvelteSnippet<[]>>;

// Helper type for DOM event handlers with typed currentTarget/target
type __SvelteEvent<Target extends EventTarget, E extends Event> = E & {
  currentTarget: Target;
  target: Target;
};

// Helper types for element attribute name checking.
type __SvelteIntrinsicElements = __SvelteHTMLElements;
type __SvelteEventProps<T> =
  T & { [K in keyof T as K extends `on:${infer E}` ? `on${E}` : never]?: T[K] };
type __SvelteElementAttributes<K extends string> =
  __SvelteEventProps<
    K extends keyof __SvelteIntrinsicElements ? __SvelteIntrinsicElements[K] : __SvelteHTMLAttributes<any>
  >;

declare function __svelte_check_element<K extends string>(
  tag: K | undefined | null,
  attrs: __SvelteElementAttributes<K>
): void;

declare const __svelte_any: any;

"#;
    output.push_str(helpers);
    builder.add_generated(helpers);

    // Emit snippet declarations for module exports (top-level only when no generics)
    let mut snippet_block = String::new();
    if has_snippets {
        snippet_block.push_str("// === SNIPPET DECLARATIONS ===\n");

        for decl in &snippet_decls {
            let helper_name = format!("__svelte_snippet_params_{}", decl.name);
            let params = transform_store_subscriptions(&decl.parameters);
            snippet_block.push_str(&format!("function {}({}) {{}}\n", helper_name, params));
            snippet_block.push_str(&format!(
                "const {}: __SvelteSnippet<Parameters<typeof {}>> = null as any;\n",
                decl.name, helper_name
            ));
        }
        snippet_block.push('\n');
    }

    if !has_generics && !snippet_block.is_empty() {
        output.push_str(&snippet_block);
        builder.add_generated(&snippet_block);
    }

    // Get the default props type for SvelteKit route files
    let default_props_type = route_kind.props_type();
    let template_result = generate_template_check_with_spans(&doc.fragment);
    let mut template_emitted = false;

    // Transform module script if present
    if let Some(module) = &doc.module_script {
        let section = "// === MODULE SCRIPT ===\n";
        output.push_str(section);
        builder.add_generated(section);

        let base_offset: u32 = module.content_span.start.into();
        let rune_result =
            transform_runes_with_options(&module.content, base_offset, default_props_type);
        let mut script_output = rune_result.output;
        if !placeholder_types.is_empty() {
            script_output = strip_placeholder_type_aliases(&script_output, &placeholder_types);
        }

        let indent = script_indent(&script_output);
        if rune_result.uses_props_accessor {
            let accessor_type = default_props_type.unwrap_or("Record<string, unknown>");
            let decl = format!(
                "{}declare const $props: __SveltePropsAccessor<{}>;\n",
                indent, accessor_type
            );
            output.push_str(&decl);
            builder.add_generated(&decl);
        }
        if let Some(aliases) = render_store_aliases(&rune_result.store_names, &indent) {
            output.push_str(&aliases);
            builder.add_generated(&aliases);
        }
        output.push_str(&script_output);
        output.push('\n');

        // Add source mapping for the script content
        // For now, use simple 1:1 mapping; in a full implementation,
        // we would incorporate rune_result.mappings for precise rune spans
        builder.add_source(module.content_span.start, &script_output);
        builder.add_generated("\n");
    }

    // Transform instance script if present
    if let Some(instance) = &doc.instance_script {
        let section = "// === INSTANCE SCRIPT ===\n";
        output.push_str(section);
        builder.add_generated(section);

        let base_offset: u32 = instance.content_span.start.into();
        let rune_result =
            transform_runes_with_options(&instance.content, base_offset, default_props_type);

        let props_info = extract_props_info(&rune_result.output, &instance.content, base_offset);
        let props_type = props_info.as_ref().map(generate_props_type);
        if let Some(ref ty) = props_type {
            exports.props_type = Some(ty.clone());
        }

        let mut script_output = rune_result.output;
        if let Some(info) = &props_info {
            if info.properties.iter().any(|prop| prop.is_rest) {
                if let Some(type_ann) = info.type_annotation.as_deref() {
                    if should_loosen_props_annotation(type_ann) {
                        script_output = loosen_props_annotation(&script_output, type_ann);
                    }
                }
            }
        }
        if !placeholder_types.is_empty() {
            script_output = strip_placeholder_type_aliases(&script_output, &placeholder_types);
        }

        let indent = script_indent(&script_output);
        let props_accessor_decl = if rune_result.uses_props_accessor {
            let accessor_type = props_type
                .as_deref()
                .or(default_props_type)
                .unwrap_or("Record<string, unknown>");
            Some(format!(
                "{}declare const $props: __SveltePropsAccessor<{}>;\n",
                indent, accessor_type
            ))
        } else {
            None
        };
        let store_aliases = render_store_aliases(&rune_result.store_names, &indent);

        if has_generics {
            let (import_block, script_body) = extract_top_level_imports(&script_output);
            if !import_block.is_empty() {
                output.push_str(&import_block);
                builder.add_generated(&import_block);
            }

            let render_start = format!("function __svelte_render{}() {{\n", generics_def_str);
            output.push_str(&render_start);
            builder.add_generated(&render_start);

            if !snippet_block.is_empty() {
                output.push_str(&snippet_block);
                builder.add_generated(&snippet_block);
            }

            if let Some(decl) = props_accessor_decl {
                output.push_str(&decl);
                builder.add_generated(&decl);
            }
            if let Some(aliases) = store_aliases {
                output.push_str(&aliases);
                builder.add_generated(&aliases);
            }
            output.push_str(&script_body);
            output.push('\n');

            builder.add_source(instance.content_span.start, &script_body);
            builder.add_generated("\n");

            if !template_result.code.is_empty() {
                output.push_str(&template_result.code);
                emit_template_with_mappings(&mut builder, &template_result);
            }
            template_emitted = true;

            let render_props_type = props_type
                .as_deref()
                .or(default_props_type)
                .unwrap_or("Record<string, unknown>");
            let return_stmt = format!(
                "return {{ props: null as any as {}, exports: {}, slots: {}, events: {} }};\n",
                render_props_type, "{}", "{}", "{}"
            );
            output.push_str(&return_stmt);
            builder.add_generated(&return_stmt);

            output.push_str("}\n");
            builder.add_generated("}\n");
        } else {
            if let Some(decl) = props_accessor_decl {
                output.push_str(&decl);
                builder.add_generated(&decl);
            }
            if let Some(aliases) = store_aliases {
                output.push_str(&aliases);
                builder.add_generated(&aliases);
            }
            output.push_str(&script_output);
            output.push('\n');

            builder.add_source(instance.content_span.start, &script_output);
            builder.add_generated("\n");
        }
    }

    // Generate template type-check block with span tracking
    if !template_emitted && !template_result.code.is_empty() {
        // Use the structured template code which properly handles component props,
        // object literals, and control flow structures
        output.push_str(&template_result.code);

        // Emit template code with proper source mappings for expressions
        emit_template_with_mappings(&mut builder, &template_result);
    }

    // Generate component export
    let export_section = "\n// === COMPONENT TYPE EXPORT ===\n";
    output.push_str(export_section);
    builder.add_generated(export_section);

    let component_name = options
        .filename
        .as_deref()
        .map(component_name_from_path)
        .unwrap_or_else(|| "Component".to_string());

    // Determine if we should use TypeScript
    let is_typescript = doc
        .instance_script
        .as_ref()
        .map(|s| s.lang == ScriptLang::TypeScript)
        .unwrap_or(false)
        || doc
            .module_script
            .as_ref()
            .map(|s| s.lang == ScriptLang::TypeScript)
            .unwrap_or(false);

    // Generate the export using ComponentExports helper
    let has_generic_render = has_generics && doc.instance_script.is_some();
    let export_line = if has_generic_render {
        let internal_name = format!("__SvelteComponent_{}_", component_name);
        let props_name = format!("__SvelteProps_{}_", component_name);
        format!(
            "type {props_name}{generics_def} = ReturnType<typeof __svelte_render{generics_ref}>[\"props\"];\n\
declare const {internal_name}: {{\n\
  {generics_def}(this: void, internals: any, props: {props_name}{generics_ref}): ReturnType<typeof __svelte_render{generics_ref}>[\"exports\"];\n\
  element?: typeof HTMLElement;\n\
  z_$$bindings?: any;\n\
}};\n\
export default {internal_name};\n",
            props_name = props_name,
            internal_name = internal_name,
            generics_def = generics_def_str,
            generics_ref = generics_ref_str
        )
    } else {
        exports.generate_export(&component_name, is_typescript)
    };

    output.push_str(&export_line);
    builder.add_generated(&export_line);

    TransformResult {
        tsx_code: output,
        source_map: builder.build(),
        exports,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use svelte_parser::parse;

    #[test]
    fn test_transform_empty() {
        let doc = parse("").document;
        let result = transform(&doc, TransformOptions::default());
        assert!(result.tsx_code.contains("SvelteComponent"));
    }

    #[test]
    fn test_transform_with_script() {
        let doc = parse("<script>let x = $state(0);</script>").document;
        let result = transform(&doc, TransformOptions::default());
        assert!(result.tsx_code.contains("let x = 0"));
    }

    #[test]
    fn test_transform_with_expression() {
        let doc = parse("<div>{value}</div>").document;
        let result = transform(&doc, TransformOptions::default());
        assert!(result.tsx_code.contains("value"));
    }

    #[test]
    fn test_transform_with_typescript() {
        let doc = parse("<script lang=\"ts\">let x: number = $state(0);</script>").document;
        let result = transform(&doc, TransformOptions::default());
        assert!(result.tsx_code.contains("let x: number = 0"));
    }

    #[test]
    fn test_transform_with_filename() {
        let doc = parse("").document;
        let result = transform(
            &doc,
            TransformOptions {
                filename: Some("Counter.svelte".to_string()),
                ..Default::default()
            },
        );
        // Uses internal name to avoid conflicts with imports
        assert!(result.tsx_code.contains("__SvelteComponent_Counter_"));
    }
}
