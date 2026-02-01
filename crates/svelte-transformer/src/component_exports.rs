//! Component export extraction for Svelte 5 runes.

use std::collections::HashSet;
use std::sync::Arc;

use swc_common::{FileName, SourceMap};
use swc_ecma_ast::{
    Decl, ExportNamedSpecifier, ExportSpecifier, Module, ModuleDecl, ModuleExportName, ModuleItem,
    Pat, VarDeclKind,
};
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedName {
    pub exported: String,
    pub local: String,
}

/// Extracts component exports from the instance script.
///
/// This looks for export declarations including:
/// - `export { ... }` named re-exports
/// - `export const/let/var name = ...` variable declarations
/// - `export function name() {}` function declarations
/// - `export class Name {}` class declarations
///
/// Ignores type-only exports and re-exports with `from`.
pub fn extract_component_exports(script: &str) -> Vec<ExportedName> {
    let Some(module) = parse_module(script) else {
        return Vec::new();
    };

    let mut exports = Vec::new();
    for item in module.body {
        match item {
            ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(named)) => {
                // Handle: export { foo, bar as baz }
                if named.src.is_some() || named.type_only {
                    continue;
                }
                for spec in named.specifiers {
                    let ExportSpecifier::Named(named) = spec else {
                        continue;
                    };
                    if named.is_type_only {
                        continue;
                    }
                    if let Some(export_name) = extract_named_export(&named) {
                        exports.push(export_name);
                    }
                }
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export_decl)) => {
                // Handle: export const/let/var, export function, export class
                match &export_decl.decl {
                    Decl::Var(var_decl) => {
                        // Skip `declare` statements (e.g., `export declare const foo: Type`)
                        if var_decl.declare {
                            continue;
                        }
                        if var_decl.kind == VarDeclKind::Const
                            || var_decl.kind == VarDeclKind::Let
                            || var_decl.kind == VarDeclKind::Var
                        {
                            for decl in &var_decl.decls {
                                if let Some(name) = extract_binding_name(&decl.name) {
                                    exports.push(ExportedName {
                                        exported: name.clone(),
                                        local: name,
                                    });
                                }
                            }
                        }
                    }
                    Decl::Fn(fn_decl) => {
                        // Handle: export function foo() {}
                        let name = fn_decl.ident.sym.to_string();
                        exports.push(ExportedName {
                            exported: name.clone(),
                            local: name,
                        });
                    }
                    Decl::Class(class_decl) => {
                        // Handle: export class Foo {}
                        let name = class_decl.ident.sym.to_string();
                        exports.push(ExportedName {
                            exported: name.clone(),
                            local: name,
                        });
                    }
                    _ => {
                        // Other declarations (TsInterface, TsTypeAlias, etc.) are type-only
                    }
                }
            }
            _ => {}
        }
    }

    exports
}

pub fn build_exports_type(exports: &[ExportedName]) -> Option<String> {
    if exports.is_empty() {
        return None;
    }

    let mut parts = Vec::new();
    let mut seen = HashSet::new();
    for export in exports {
        if !is_valid_identifier(&export.local) {
            continue;
        }
        if !seen.insert(export.exported.as_str()) {
            continue;
        }
        let prop_name = if is_valid_identifier(&export.exported) {
            export.exported.clone()
        } else {
            format!("\"{}\"", escape_string_literal(&export.exported))
        };
        parts.push(format!("{}: typeof {}", prop_name, export.local));
    }

    if parts.is_empty() {
        None
    } else {
        Some(format!("{{ {} }}", parts.join(", ")))
    }
}

fn parse_module(script: &str) -> Option<Module> {
    let cm: Arc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom("svelte-instance-script".into()).into(),
        script.to_string(),
    );
    let syntax = Syntax::Typescript(TsSyntax {
        tsx: false,
        ..Default::default()
    });
    let mut parser = Parser::new(syntax, StringInput::from(&*fm), None);
    parser.parse_module().ok()
}

fn extract_named_export(named: &ExportNamedSpecifier) -> Option<ExportedName> {
    let local = module_export_name_to_ident(&named.orig)?;
    let exported = named
        .exported
        .as_ref()
        .map(module_export_name_to_string)
        .unwrap_or_else(|| local.clone());

    Some(ExportedName { exported, local })
}

fn module_export_name_to_ident(name: &ModuleExportName) -> Option<String> {
    match name {
        ModuleExportName::Ident(ident) => Some(ident.sym.to_string()),
        ModuleExportName::Str(_) => None,
    }
}

/// Extracts the binding name from a pattern.
/// Only handles simple identifier patterns; destructuring patterns are ignored.
fn extract_binding_name(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(ident) => Some(ident.id.sym.to_string()),
        // Destructuring patterns (Array, Object) are not supported as component exports
        _ => None,
    }
}

fn module_export_name_to_string(name: &ModuleExportName) -> String {
    match name {
        ModuleExportName::Ident(ident) => ident.sym.to_string(),
        ModuleExportName::Str(value) => value.value.to_string_lossy().into_owned(),
    }
}

fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn escape_string_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_simple_exports() {
        let script = "let count = 0; export { count, name };";
        let exports = extract_component_exports(script);
        assert_eq!(exports.len(), 2);
        assert!(exports
            .iter()
            .any(|e| e.exported == "count" && e.local == "count"));
        assert!(exports
            .iter()
            .any(|e| e.exported == "name" && e.local == "name"));
    }

    #[test]
    fn extracts_aliased_exports() {
        let script = "let count = 0; export { count as total };";
        let exports = extract_component_exports(script);
        assert_eq!(
            exports,
            vec![ExportedName {
                exported: "total".to_string(),
                local: "count".to_string(),
            }]
        );
    }

    #[test]
    fn ignores_type_only_exports() {
        let script = "export type { Foo }; export { count };";
        let exports = extract_component_exports(script);
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].exported, "count");
    }

    #[test]
    fn ignores_reexports() {
        let script = "export { count } from './module'; export { name };";
        let exports = extract_component_exports(script);
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].exported, "name");
    }

    #[test]
    fn extracts_export_const() {
        let script = "export const snapshot = { capture: () => {} };";
        let exports = extract_component_exports(script);
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].exported, "snapshot");
        assert_eq!(exports[0].local, "snapshot");
    }

    #[test]
    fn extracts_export_let() {
        let script = "export let value = 42;";
        let exports = extract_component_exports(script);
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].exported, "value");
    }

    #[test]
    fn extracts_export_function() {
        let script = "export function greet(name: string) { return `Hello, ${name}`; }";
        let exports = extract_component_exports(script);
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].exported, "greet");
    }

    #[test]
    fn extracts_export_class() {
        let script = "export class Counter { count = 0; }";
        let exports = extract_component_exports(script);
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].exported, "Counter");
    }

    #[test]
    fn extracts_multiple_const_declarations() {
        let script = "export const a = 1, b = 2;";
        let exports = extract_component_exports(script);
        assert_eq!(exports.len(), 2);
        assert!(exports.iter().any(|e| e.exported == "a"));
        assert!(exports.iter().any(|e| e.exported == "b"));
    }

    #[test]
    fn extracts_mixed_exports() {
        let script = r#"
            export const snapshot = {};
            export function reset() {}
            let internal = 0;
            export { internal as state };
        "#;
        let exports = extract_component_exports(script);
        assert_eq!(exports.len(), 3);
        assert!(exports.iter().any(|e| e.exported == "snapshot"));
        assert!(exports.iter().any(|e| e.exported == "reset"));
        assert!(exports
            .iter()
            .any(|e| e.exported == "state" && e.local == "internal"));
    }

    #[test]
    fn builds_exports_type() {
        let exports = vec![
            ExportedName {
                exported: "count".to_string(),
                local: "count".to_string(),
            },
            ExportedName {
                exported: "name".to_string(),
                local: "name".to_string(),
            },
        ];
        let ty = build_exports_type(&exports).unwrap();
        assert!(ty.contains("count: typeof count"));
        assert!(ty.contains("name: typeof name"));
    }
}
