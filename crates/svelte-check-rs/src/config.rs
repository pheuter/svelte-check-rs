//! Configuration loading.

use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use swc_common::SourceMap;
use swc_ecma_ast::{
    ArrayLit, AssignExpr, AssignOp, AssignTarget, CallExpr, Callee, Decl, ExportDefaultExpr, Expr,
    ExprStmt, KeyValueProp, Lit, MemberExpr, MemberProp, ModuleDecl, ModuleItem, ObjectLit, Pat,
    Prop, PropName, PropOrSpread, SimpleAssignTarget, Stmt, VarDeclKind,
};
use swc_ecma_parser::{parse_file_as_module, EsSyntax, Syntax, TsSyntax};

/// Extensions svelte-check-rs natively understands.
///
/// Longer suffixes first so a filename like `foo.svelte.ts` matches
/// `.svelte.ts` rather than `.svelte`.
const NATIVE_EXTENSIONS: &[&str] = &[".svelte.ts", ".svelte.js", ".svelte"];

/// The kind of Svelte file being processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvelteFileKind {
    /// A `.svelte` component file with HTML template, script, and styles.
    Component,
    /// A `.svelte.ts` or `.svelte.js` module file with runes but no template.
    Module,
}

impl SvelteFileKind {
    /// Determines the file kind from a file path.
    pub fn from_path(path: &Utf8Path) -> Option<Self> {
        let file_name = path.file_name()?;
        Self::from_filename(file_name)
    }

    /// Determines the file kind from a filename.
    pub fn from_filename(filename: &str) -> Option<Self> {
        if filename.ends_with(".svelte.ts") || filename.ends_with(".svelte.js") {
            Some(Self::Module)
        } else if filename.ends_with(".svelte") {
            Some(Self::Component)
        } else {
            None
        }
    }

    /// Returns true if this is a module file (`.svelte.ts` or `.svelte.js`).
    #[allow(dead_code)]
    pub fn is_module(&self) -> bool {
        matches!(self, Self::Module)
    }

    /// Returns true if this is a component file (`.svelte`).
    #[allow(dead_code)]
    pub fn is_component(&self) -> bool {
        matches!(self, Self::Component)
    }
}

/// Svelte project configuration.
#[derive(Debug, Clone, Default)]
pub struct SvelteConfig {
    /// File extensions to process.
    pub extensions: Vec<String>,

    /// Files/patterns to exclude.
    #[allow(dead_code)]
    pub exclude: Vec<String>,

    /// SvelteKit configuration.
    pub kit: KitConfig,

    /// Compiler options.
    pub compiler_options: SvelteCompilerOptions,
}

/// SvelteKit-specific configuration.
#[derive(Debug, Clone, Default)]
pub struct KitConfig {
    /// Path aliases (e.g., `$lib` -> `./src/lib`).
    pub alias: HashMap<String, String>,
}

/// Svelte compiler options.
#[derive(Debug, Clone, Default)]
pub struct SvelteCompilerOptions {
    /// Enable runes mode.
    pub runes: Option<bool>,

    /// `compilerOptions.experimental.async` from `svelte.config.js`.
    pub experimental_async: Option<bool>,
}

impl SvelteConfig {
    /// Loads configuration from a `vite.config` or `svelte.config` file.
    ///
    /// Precedence mirrors upstream language-tools (commit 5b13da15, #3031):
    /// `vite.config.{js,mjs,ts,cjs,mts,cts}` is probed first and preferred when
    /// it yields any Svelte plugin options; otherwise we fall through to
    /// `svelte.config.{js,ts,cjs,mjs,mts}`.
    ///
    /// CRITICAL DIVERGENCE FROM UPSTREAM: upstream runs `vite.resolveConfig(...)`
    /// at RUNTIME and reads the resolved plugin's `api.options` (the
    /// `vite-plugin-sveltekit-setup` or `vite-plugin-svelte:config` plugin).
    /// svelte-check-rs parses STATICALLY with SWC and cannot execute vite, so
    /// `parse_vite_config` is a best-effort static approximation that handles the
    /// common literal case (`svelte({...})` / `sveltekit({...})` inside a
    /// `plugins: [...]` array). Dynamic/computed plugin options (spreads,
    /// conditionals, function-returned options, options imported from other
    /// modules) are not statically extractable and correctly fall through to
    /// `svelte.config` or defaults — the same static-parse tradeoff already used
    /// for `svelte.config` (#3009).
    pub fn load(project_root: &Utf8Path) -> Self {
        // Probe vite.config first (upstream VITE_CONFIG_EXTENSIONS order:
        // js -> mjs -> ts -> cjs -> mts -> cts); first existing file wins.
        let vite_config_files = [
            "vite.config.js",
            "vite.config.mjs",
            "vite.config.ts",
            "vite.config.cjs",
            "vite.config.mts",
            "vite.config.cts",
        ];

        for vite_config_file in vite_config_files {
            let config_path = project_root.join(vite_config_file);
            if config_path.exists() {
                match Self::parse_vite_config(&config_path) {
                    // vite.config yielded svelte/sveltekit plugin options: prefer it.
                    Ok(Some(config)) => return config,
                    // vite.config present but no usable plugin options (e.g. a
                    // bare `sveltekit()` with no args): fall through to
                    // svelte.config, matching upstream's vite-then-svelte
                    // precedence.
                    Ok(None) => {}
                    // Parse error: warn and fall through to svelte.config
                    // (mirrors the svelte.config error handling, but does NOT
                    // abort to default() — svelte.config may still be valid).
                    Err(e) => {
                        eprintln!("Warning: Failed to parse {}: {}", config_path, e);
                    }
                }
                break;
            }
        }

        // Try multiple config file names. Order mirrors upstream language-tools
        // (js -> ts -> cjs -> mjs -> mts); first match wins.
        //
        // Unlike upstream, we do NOT gate the TypeScript extensions (`.ts`,
        // `.mts`) behind `process.features.typescript`: upstream needs Node to
        // import the config at runtime, whereas svelte-check-rs parses configs
        // statically via SWC, so the full set is always available.
        let config_files = [
            "svelte.config.js",
            "svelte.config.ts",
            "svelte.config.cjs",
            "svelte.config.mjs",
            "svelte.config.mts",
        ];

        for config_file in config_files {
            let config_path = project_root.join(config_file);
            if config_path.exists() {
                match Self::parse_config(&config_path) {
                    Ok(config) => return config,
                    Err(e) => {
                        eprintln!("Warning: Failed to parse {}: {}", config_path, e);
                        return Self::default();
                    }
                }
            }
        }

        Self::default()
    }

    /// Parses a `svelte.config.{js,ts,cjs,mjs,mts}` file using SWC.
    ///
    /// Supports both ESM (`export default { ... }` / `export default config`)
    /// and CommonJS (`module.exports = { ... }` / `exports = { ... }`, and the
    /// identifier forms `module.exports = config`). CommonJS assignments are fed
    /// through the same extraction path as `export default`.
    fn parse_config(path: &Utf8Path) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;

        let cm: Arc<SourceMap> = Default::default();
        let fm = cm.new_source_file(
            swc_common::FileName::Custom(path.to_string()).into(),
            content,
        );

        // Use TypeScript syntax for .ts/.mts files, ES for .js/.cjs/.mjs.
        // Note: `"svelte.config.mts".ends_with(".ts")` is false (it ends with
        // `.mts`), so `.mts` must be matched explicitly or it would wrongly
        // parse with the ES branch and choke on TS-only syntax.
        let syntax = if path.as_str().ends_with(".ts") || path.as_str().ends_with(".mts") {
            Syntax::Typescript(TsSyntax {
                tsx: false,
                ..Default::default()
            })
        } else {
            Syntax::Es(EsSyntax {
                jsx: false,
                ..Default::default()
            })
        };

        let module = parse_file_as_module(
            &fm,
            syntax,
            swc_ecma_ast::EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .map_err(|e| format!("Parse error: {:?}", e))?;

        let mut config = SvelteConfig::default();

        let object_by_name = Self::collect_top_level_object_consts(&module.body);

        // Find the config object. Supports:
        //   - ESM `export default { ... }` / `export default config`
        //   - CommonJS `module.exports = { ... }` / `exports = { ... }`
        //     and the identifier forms `module.exports = config`.
        // Plus TS forms where the object/identifier on the RHS is wrapped in a
        // type assertion (`... satisfies Config`, `... as Config`, `... as
        // const`, `<Config>...`, `...!`) — all transparent at runtime.
        //
        // Items are processed in source order; if a file mixes `export default`
        // and `module.exports` (e.g. transpiled output), both contribute and
        // later writes win for overlapping keys.
        for item in &module.body {
            match item {
                ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(ExportDefaultExpr {
                    expr,
                    ..
                })) => {
                    Self::extract_from_rhs(expr.as_ref(), &object_by_name, &mut config);
                }
                ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) => {
                    if let Expr::Assign(AssignExpr {
                        op: AssignOp::Assign,
                        left,
                        right,
                        ..
                    }) = expr.as_ref()
                    {
                        if Self::is_commonjs_exports_target(left) {
                            Self::extract_from_rhs(right.as_ref(), &object_by_name, &mut config);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(config)
    }

    /// Parses a `vite.config.{js,mjs,ts,cjs,mts,cts}` file using SWC and
    /// statically extracts Svelte plugin options.
    ///
    /// STATIC APPROXIMATION (see `SvelteConfig::load` for the full divergence
    /// note): upstream runs `vite.resolveConfig(...)` at runtime and reads the
    /// resolved plugin's `api.options`. We cannot execute vite, so we resolve the
    /// default-export (or CommonJS `module.exports`) object, unwrap a
    /// `defineConfig(...)` wrapper, find the `plugins` array, scan it for a
    /// `svelte({...})` or `sveltekit({...})` call, and feed the call's first
    /// argument object literal through the existing `extract_config_from_object`
    /// helper (so `compilerOptions.runes`, `compilerOptions.experimental.async`,
    /// `extensions`, and `kit` are honored).
    ///
    /// Fall-through contract:
    ///   - `Ok(Some(cfg))` when a `svelte`/`sveltekit` call supplied an options
    ///     object — `load()` prefers this and does NOT consult `svelte.config`
    ///     (faithful to upstream's all-or-nothing precedence; no merge).
    ///   - `Ok(None)` when no `svelte`/`sveltekit` plugin call had an options
    ///     object (the common bare `sveltekit()` case) — `load()` then falls
    ///     through to `svelte.config`. This is essential: fixtures like
    ///     `sveltekit-bundler` use a bare `sveltekit()` in vite.config but keep
    ///     their real config (including `experimental.async`) in svelte.config.js.
    ///   - `Err(..)` on parse failure — `load()` warns and falls through to
    ///     `svelte.config` (it does NOT abort to `default()`).
    fn parse_vite_config(path: &Utf8Path) -> Result<Option<Self>, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;

        let cm: Arc<SourceMap> = Default::default();
        let fm = cm.new_source_file(
            swc_common::FileName::Custom(path.to_string()).into(),
            content,
        );

        // TypeScript syntax for .ts/.mts/.cts, ES for .js/.cjs/.mjs.
        // Note: `.mts`/`.cts` do not end with `.ts`, so they must be matched
        // explicitly or they would wrongly select the ES branch.
        let p = path.as_str();
        let syntax = if p.ends_with(".ts") || p.ends_with(".mts") || p.ends_with(".cts") {
            Syntax::Typescript(TsSyntax {
                tsx: false,
                ..Default::default()
            })
        } else {
            Syntax::Es(EsSyntax {
                jsx: false,
                ..Default::default()
            })
        };

        let module = parse_file_as_module(
            &fm,
            syntax,
            swc_ecma_ast::EsVersion::Es2022,
            None,
            &mut Vec::new(),
        )
        .map_err(|e| format!("Parse error: {:?}", e))?;

        let object_by_name = Self::collect_top_level_object_consts(&module.body);

        // Resolve the root config object from `export default ...` or a CommonJS
        // `module.exports = ...` / `exports = ...` assignment.
        let mut root_object: Option<&ObjectLit> = None;
        for item in &module.body {
            match item {
                ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(ExportDefaultExpr {
                    expr,
                    ..
                })) => {
                    if let Some(obj) =
                        Self::resolve_vite_root_object(expr.as_ref(), &object_by_name)
                    {
                        root_object = Some(obj);
                        break;
                    }
                }
                ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) => {
                    if let Expr::Assign(AssignExpr {
                        op: AssignOp::Assign,
                        left,
                        right,
                        ..
                    }) = expr.as_ref()
                    {
                        if Self::is_commonjs_exports_target(left) {
                            if let Some(obj) =
                                Self::resolve_vite_root_object(right.as_ref(), &object_by_name)
                            {
                                root_object = Some(obj);
                                break;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let Some(root_object) = root_object else {
            return Ok(None);
        };

        // Find the `plugins: [...]` array property.
        let Some(plugins) = Self::find_plugins_array(root_object) else {
            return Ok(None);
        };

        // Scan plugins for a `svelte({...})` / `sveltekit({...})` call and feed
        // its first-argument options object through the shared extractor.
        let mut config = SvelteConfig::default();
        let mut found_options = false;
        for elem in plugins.elems.iter().flatten() {
            let Expr::Call(call) = Self::unwrap_ts_assertions(elem.expr.as_ref()) else {
                continue;
            };
            if !Self::is_svelte_plugin_call(call) {
                continue;
            }
            // First argument is the plugin options object.
            let Some(first_arg) = call.args.first() else {
                continue;
            };
            let inner = Self::unwrap_ts_assertions(first_arg.expr.as_ref());
            let options_obj = match inner {
                Expr::Object(obj) => Some(obj),
                Expr::Ident(ident) => object_by_name.get(ident.sym.as_ref()).copied(),
                _ => None,
            };
            if let Some(options_obj) = options_obj {
                Self::extract_config_from_object(options_obj, &mut config);
                found_options = true;
            }
        }

        if found_options {
            Ok(Some(config))
        } else {
            // No svelte/sveltekit plugin supplied an options object (e.g. bare
            // `sveltekit()`): fall through to svelte.config.
            Ok(None)
        }
    }

    /// Resolves a vite config root expression to its object literal.
    ///
    /// Handles, in order: a TS-assertion wrapper (peeled first), a
    /// `defineConfig(<obj>)` / `defineConfig(<ident>)` call wrapper, a direct
    /// object literal, and an identifier referring to a top-level
    /// `const = { ... }`. Returns `None` (never panics) for any other shape.
    fn resolve_vite_root_object<'a>(
        expr: &'a Expr,
        object_by_name: &HashMap<&'a str, &'a ObjectLit>,
    ) -> Option<&'a ObjectLit> {
        let inner = Self::unwrap_ts_assertions(expr);
        match inner {
            Expr::Object(obj) => Some(obj),
            Expr::Ident(ident) => object_by_name.get(ident.sym.as_ref()).copied(),
            // `defineConfig({ ... })` / `defineConfig(config)` wrapper.
            Expr::Call(call) => {
                if !Self::is_callee_ident(call, "defineConfig") {
                    return None;
                }
                let first_arg = call.args.first()?;
                let arg_inner = Self::unwrap_ts_assertions(first_arg.expr.as_ref());
                match arg_inner {
                    Expr::Object(obj) => Some(obj),
                    Expr::Ident(ident) => object_by_name.get(ident.sym.as_ref()).copied(),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Finds the `plugins` property of a vite config object when it is an array
    /// literal.
    fn find_plugins_array(obj: &ObjectLit) -> Option<&ArrayLit> {
        for prop in &obj.props {
            if let PropOrSpread::Prop(prop) = prop {
                if let Prop::KeyValue(KeyValueProp { key, value }) = prop.as_ref() {
                    if Self::prop_name_str(key) == Some("plugins") {
                        if let Expr::Array(arr) = value.as_ref() {
                            return Some(arr);
                        }
                    }
                }
            }
        }
        None
    }

    /// Returns true if `call`'s callee is the bare identifier `svelte` or
    /// `sveltekit` (the vite-plugin-svelte / SvelteKit plugin factories).
    ///
    /// Matching is conservative: only bare identifiers are accepted, so
    /// member-expr callees (`foo.svelte()`) and unrelated calls are skipped. This
    /// favors false negatives (fall through to svelte.config) over false
    /// positives.
    fn is_svelte_plugin_call(call: &CallExpr) -> bool {
        Self::is_callee_ident(call, "svelte") || Self::is_callee_ident(call, "sveltekit")
    }

    /// Returns true if `call`'s callee is the bare identifier `name`.
    fn is_callee_ident(call: &CallExpr, name: &str) -> bool {
        match &call.callee {
            Callee::Expr(expr) => {
                matches!(expr.as_ref(), Expr::Ident(ident) if ident.sym.as_ref() == name)
            }
            Callee::Super(_) | Callee::Import(_) => false,
        }
    }

    /// Returns true if `target` is the CommonJS exports object, i.e. either
    /// `module.exports` (member-expr) or a bare `exports` identifier.
    ///
    /// We only care about these two specific shapes, so the matches against the
    /// large `SimpleAssignTarget` (11 variants) and `MemberProp` (3 variants)
    /// enums fall through to `false` for everything else (any other assignment
    /// target is simply not a config definition and is ignored).
    fn is_commonjs_exports_target(target: &AssignTarget) -> bool {
        let AssignTarget::Simple(simple) = target else {
            return false;
        };
        match simple {
            // `module.exports = ...`
            SimpleAssignTarget::Member(MemberExpr { obj, prop, .. }) => {
                let Expr::Ident(obj_ident) = obj.as_ref() else {
                    return false;
                };
                if obj_ident.sym.as_ref() != "module" {
                    return false;
                }
                matches!(prop, MemberProp::Ident(name) if name.sym.as_ref() == "exports")
            }
            // bare `exports = ...`
            SimpleAssignTarget::Ident(binding) => binding.id.sym.as_ref() == "exports",
            SimpleAssignTarget::SuperProp(_)
            | SimpleAssignTarget::Paren(_)
            | SimpleAssignTarget::OptChain(_)
            | SimpleAssignTarget::TsAs(_)
            | SimpleAssignTarget::TsSatisfies(_)
            | SimpleAssignTarget::TsNonNull(_)
            | SimpleAssignTarget::TsTypeAssertion(_)
            | SimpleAssignTarget::TsInstantiation(_)
            | SimpleAssignTarget::Invalid(_) => false,
        }
    }

    /// Resolves the right-hand side of a config definition (the body of `export
    /// default` or a CommonJS `module.exports = ...` assignment) to an object
    /// literal and extracts configuration from it.
    ///
    /// Shared by both the ESM and CommonJS code paths so they use an identical
    /// resolution strategy: peel TS assertions, then accept either a direct
    /// object literal or an identifier referring to a top-level `const = { ... }`.
    fn extract_from_rhs(
        expr: &Expr,
        object_by_name: &HashMap<&str, &ObjectLit>,
        config: &mut SvelteConfig,
    ) {
        let inner = Self::unwrap_ts_assertions(expr);
        let root = match inner {
            Expr::Object(obj) => Some(obj),
            Expr::Ident(ident) => object_by_name.get(ident.sym.as_ref()).copied(),
            _ => None,
        };
        if let Some(obj) = root {
            Self::extract_config_from_object(obj, config);
        }
    }

    /// Peels TypeScript type-assertion wrappers that are transparent at runtime
    /// (`satisfies`, `as`, `as const`, `<T>expr`, and non-null `!`) so the
    /// underlying object/identifier can be resolved from a `.ts`/`.mts` config.
    fn unwrap_ts_assertions(expr: &Expr) -> &Expr {
        match expr {
            Expr::TsSatisfies(e) => Self::unwrap_ts_assertions(e.expr.as_ref()),
            Expr::TsAs(e) => Self::unwrap_ts_assertions(e.expr.as_ref()),
            Expr::TsConstAssertion(e) => Self::unwrap_ts_assertions(e.expr.as_ref()),
            Expr::TsTypeAssertion(e) => Self::unwrap_ts_assertions(e.expr.as_ref()),
            Expr::TsNonNull(e) => Self::unwrap_ts_assertions(e.expr.as_ref()),
            other => other,
        }
    }

    /// Maps `const name = { ... }` at module top level to the object literal (for resolving
    /// `export default name` in SvelteKit configs).
    fn collect_top_level_object_consts(body: &[ModuleItem]) -> HashMap<&str, &ObjectLit> {
        let mut out = HashMap::new();
        for item in body {
            let ModuleItem::Stmt(Stmt::Decl(Decl::Var(var))) = item else {
                continue;
            };
            if var.kind != VarDeclKind::Const {
                continue;
            }
            for decl in &var.decls {
                let Pat::Ident(binding) = &decl.name else {
                    continue;
                };
                let Some(init) = decl.init.as_ref().map(|e| e.as_ref()) else {
                    continue;
                };
                // Unwrap TS assertions like `const config = { ... } satisfies Config`.
                let Expr::Object(obj) = Self::unwrap_ts_assertions(init) else {
                    continue;
                };
                out.insert(binding.id.sym.as_str(), obj);
            }
        }
        out
    }

    /// Gets a string value from a PropName.
    fn prop_name_str(key: &PropName) -> Option<&str> {
        match key {
            PropName::Ident(ident) => Some(ident.sym.as_str()),
            PropName::Str(s) => s.value.as_str(),
            _ => None,
        }
    }

    /// Gets a string value from a Str literal.
    fn str_value(s: &swc_ecma_ast::Str) -> Option<&str> {
        s.value.as_str()
    }

    /// Extracts configuration from an object literal.
    fn extract_config_from_object(obj: &ObjectLit, config: &mut SvelteConfig) {
        for prop in &obj.props {
            if let PropOrSpread::Prop(prop) = prop {
                if let Prop::KeyValue(KeyValueProp { key, value }) = prop.as_ref() {
                    let Some(key_name) = Self::prop_name_str(key) else {
                        continue;
                    };

                    match key_name {
                        "kit" => {
                            if let Expr::Object(kit_obj) = value.as_ref() {
                                Self::extract_kit_config(kit_obj, config);
                            }
                        }
                        "compilerOptions" => {
                            if let Expr::Object(opts_obj) = value.as_ref() {
                                Self::extract_compiler_options(opts_obj, config);
                            }
                        }
                        "extensions" => {
                            if let Expr::Array(arr) = value.as_ref() {
                                for elem in arr.elems.iter().flatten() {
                                    if let Expr::Lit(Lit::Str(s)) = elem.expr.as_ref() {
                                        if let Some(ext) = Self::str_value(s) {
                                            config.extensions.push(ext.to_string());
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Extracts kit configuration.
    fn extract_kit_config(obj: &ObjectLit, config: &mut SvelteConfig) {
        for prop in &obj.props {
            if let PropOrSpread::Prop(prop) = prop {
                if let Prop::KeyValue(KeyValueProp { key, value }) = prop.as_ref() {
                    let Some(key_name) = Self::prop_name_str(key) else {
                        continue;
                    };

                    if key_name == "alias" {
                        if let Expr::Object(alias_obj) = value.as_ref() {
                            Self::extract_aliases(alias_obj, config);
                        }
                    }
                }
            }
        }
    }

    /// Extracts path aliases from the alias object.
    fn extract_aliases(obj: &ObjectLit, config: &mut SvelteConfig) {
        for prop in &obj.props {
            if let PropOrSpread::Prop(prop) = prop {
                if let Prop::KeyValue(KeyValueProp { key, value }) = prop.as_ref() {
                    let Some(alias_name) = Self::prop_name_str(key) else {
                        continue;
                    };

                    if let Expr::Lit(Lit::Str(s)) = value.as_ref() {
                        if let Some(path) = Self::str_value(s) {
                            config
                                .kit
                                .alias
                                .insert(alias_name.to_string(), path.to_string());
                        }
                    }
                }
            }
        }
    }

    /// Extracts compiler options.
    fn extract_compiler_options(obj: &ObjectLit, config: &mut SvelteConfig) {
        for prop in &obj.props {
            if let PropOrSpread::Prop(prop) = prop {
                if let Prop::KeyValue(KeyValueProp { key, value }) = prop.as_ref() {
                    let Some(key_name) = Self::prop_name_str(key) else {
                        continue;
                    };

                    if key_name == "runes" {
                        if let Expr::Lit(Lit::Bool(b)) = value.as_ref() {
                            config.compiler_options.runes = Some(b.value);
                        }
                    } else if key_name == "experimental" {
                        if let Expr::Object(exp_obj) = value.as_ref() {
                            Self::extract_compiler_experimental(exp_obj, config);
                        }
                    }
                }
            }
        }
    }

    fn extract_compiler_experimental(obj: &ObjectLit, config: &mut SvelteConfig) {
        for prop in &obj.props {
            if let PropOrSpread::Prop(prop) = prop {
                if let Prop::KeyValue(KeyValueProp { key, value }) = prop.as_ref() {
                    let Some(key_name) = Self::prop_name_str(key) else {
                        continue;
                    };
                    if key_name == "async" {
                        if let Expr::Lit(Lit::Bool(b)) = value.as_ref() {
                            config.compiler_options.experimental_async = Some(b.value);
                        }
                    }
                }
            }
        }
    }

    /// Returns the file extensions to walk during discovery.
    ///
    /// Always includes the natively-supported extensions:
    /// - `.svelte` - Component files
    /// - `.svelte.ts` - TypeScript module files with runes
    /// - `.svelte.js` - JavaScript module files with runes
    ///
    /// Any extra extensions declared in `svelte.config.js` (e.g. `.svx` from
    /// mdsvex) are appended so they are still discovered and reported, but the
    /// orchestrator filters out the unrecognized ones with a user-facing
    /// warning rather than feeding them into the type-checker.
    ///
    /// Order matters: longer suffixes must come before `.svelte` so that
    /// `.svelte.ts` matches before `.svelte`.
    pub fn file_extensions(&self) -> Vec<&str> {
        let mut extensions: Vec<&str> = NATIVE_EXTENSIONS.to_vec();
        for ext in &self.extensions {
            let s = ext.as_str();
            if !extensions.contains(&s) {
                extensions.push(s);
            }
        }
        extensions
    }

    /// Returns extensions declared in `svelte.config.js` that we don't
    /// natively support. Files with these extensions are discovered (so we can
    /// report them) but skipped from the rest of the pipeline.
    pub fn unsupported_extensions(&self) -> Vec<&str> {
        self.extensions
            .iter()
            .map(|s| s.as_str())
            .filter(|s| !NATIVE_EXTENSIONS.contains(s))
            .collect()
    }

    /// Returns whether runes mode is enabled (defaults to true for Svelte 5).
    #[allow(dead_code)]
    pub fn runes_enabled(&self) -> bool {
        self.compiler_options.runes.unwrap_or(true)
    }
}

/// TypeScript configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TsConfig {
    /// Compiler options.
    #[serde(default)]
    pub compiler_options: CompilerOptions,

    /// Include patterns.
    #[serde(default)]
    #[allow(dead_code)]
    pub include: Vec<String>,

    /// Exclude patterns (used to filter out files from checking).
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// TypeScript compiler options.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct CompilerOptions {
    /// Target ECMAScript version.
    pub target: Option<String>,

    /// Module system.
    pub module: Option<String>,

    /// Module resolution strategy.
    pub module_resolution: Option<String>,

    /// Enable strict mode.
    #[serde(default)]
    pub strict: bool,

    /// Base URL for module resolution.
    pub base_url: Option<String>,

    /// Path mappings.
    #[serde(default)]
    pub paths: HashMap<String, Vec<String>>,
}

impl CompilerOptions {
    /// Returns true if the module resolution strategy requires explicit file extensions
    /// for relative imports (NodeNext, Node16).
    pub fn requires_explicit_extensions(&self) -> bool {
        // Check moduleResolution first, then fall back to module
        // (when module is NodeNext/Node16, moduleResolution defaults to the same)
        let resolution = self
            .module_resolution
            .as_deref()
            .or(self.module.as_deref())
            .unwrap_or("");

        matches!(resolution.to_lowercase().as_str(), "nodenext" | "node16")
    }
}

impl TsConfig {
    /// Loads configuration from a tsconfig.json file.
    pub fn load(path: &Utf8Path) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;

        // Remove comments (simple approach, doesn't handle strings)
        let content = remove_json_comments(&content);

        serde_json::from_str(&content).ok()
    }

    /// Finds and loads tsconfig.json from a project root.
    pub fn find(project_root: &Utf8Path) -> Option<(Utf8PathBuf, Self)> {
        let path = project_root.join("tsconfig.json");
        if path.exists() {
            Self::load(&path).map(|config| (path, config))
        } else {
            None
        }
    }

    /// Merges SvelteKit aliases into the paths configuration.
    #[allow(dead_code)]
    pub fn merge_svelte_aliases(&mut self, svelte_config: &SvelteConfig) {
        for (alias, path) in &svelte_config.kit.alias {
            // Convert SvelteKit alias format to TypeScript paths format
            // e.g., "$lib" -> "$lib/*" mapping to ["./src/lib/*"]
            let ts_alias = if alias.ends_with("/*") {
                alias.clone()
            } else {
                format!("{}/*", alias)
            };

            let ts_path = if path.ends_with("/*") {
                path.clone()
            } else {
                format!("{}/*", path)
            };

            self.compiler_options
                .paths
                .entry(ts_alias)
                .or_insert_with(|| vec![ts_path]);
        }
    }
}

/// Removes single-line and multi-line comments from JSON.
fn remove_json_comments(json: &str) -> String {
    let mut result = String::with_capacity(json.len());
    let mut chars = json.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            result.push(c);
            if c == '"' {
                in_string = false;
            } else if c == '\\' {
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            }
        } else if c == '"' {
            result.push(c);
            in_string = true;
        } else if c == '/' {
            match chars.peek() {
                Some('/') => {
                    // Single-line comment
                    chars.next();
                    while let Some(&next) = chars.peek() {
                        if next == '\n' {
                            break;
                        }
                        chars.next();
                    }
                }
                Some('*') => {
                    // Multi-line comment
                    chars.next();
                    while let Some(next) = chars.next() {
                        if next == '*' && chars.peek() == Some(&'/') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {
                    result.push(c);
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_comments() {
        let json = r#"{
            // This is a comment
            "key": "value" /* inline comment */
        }"#;

        let cleaned = remove_json_comments(json);
        assert!(!cleaned.contains("//"));
        assert!(!cleaned.contains("/*"));
        assert!(cleaned.contains("\"key\""));
    }

    #[test]
    fn test_default_extensions() {
        let config = SvelteConfig::default();
        assert_eq!(
            config.file_extensions(),
            vec![".svelte.ts", ".svelte.js", ".svelte"]
        );
    }

    #[test]
    fn test_svelte_file_kind() {
        // Component files
        assert_eq!(
            SvelteFileKind::from_filename("App.svelte"),
            Some(SvelteFileKind::Component)
        );
        assert_eq!(
            SvelteFileKind::from_filename("Counter.svelte"),
            Some(SvelteFileKind::Component)
        );

        // Module files
        assert_eq!(
            SvelteFileKind::from_filename("counter.svelte.ts"),
            Some(SvelteFileKind::Module)
        );
        assert_eq!(
            SvelteFileKind::from_filename("state.svelte.js"),
            Some(SvelteFileKind::Module)
        );

        // Not Svelte files
        assert_eq!(SvelteFileKind::from_filename("app.ts"), None);
        assert_eq!(SvelteFileKind::from_filename("app.js"), None);
        assert_eq!(SvelteFileKind::from_filename("README.md"), None);
    }

    #[test]
    fn test_svelte_file_kind_from_path() {
        use camino::Utf8Path;

        assert_eq!(
            SvelteFileKind::from_path(Utf8Path::new("src/lib/App.svelte")),
            Some(SvelteFileKind::Component)
        );
        assert_eq!(
            SvelteFileKind::from_path(Utf8Path::new("src/lib/counter.svelte.ts")),
            Some(SvelteFileKind::Module)
        );
        assert_eq!(
            SvelteFileKind::from_path(Utf8Path::new("src/lib/utils.ts")),
            None
        );
    }

    #[test]
    fn test_runes_default_enabled() {
        let config = SvelteConfig::default();
        assert!(config.runes_enabled());
    }

    #[test]
    fn test_merge_svelte_aliases() {
        let svelte_config = SvelteConfig {
            kit: KitConfig {
                alias: HashMap::from([
                    ("$lib".to_string(), "./src/lib".to_string()),
                    ("$components".to_string(), "./src/components".to_string()),
                ]),
            },
            ..Default::default()
        };

        let mut ts_config = TsConfig::default();
        ts_config.merge_svelte_aliases(&svelte_config);

        assert!(ts_config.compiler_options.paths.contains_key("$lib/*"));
        assert!(ts_config
            .compiler_options
            .paths
            .contains_key("$components/*"));
    }

    #[test]
    fn test_parse_sveltekit_bundler_config_includes_experimental_async() {
        let path = Utf8Path::new("../../test-fixtures/projects/sveltekit-bundler");
        let config = SvelteConfig::load(path);
        assert_eq!(
            config.compiler_options.experimental_async,
            Some(true),
            "expected const config + export default to resolve compilerOptions.experimental.async"
        );
    }

    #[test]
    fn test_parse_svelte_config_js() {
        // Parse the test fixture svelte.config.js
        let path = Utf8Path::new("../../test-fixtures/projects/simple-app");
        let config = SvelteConfig::load(path);

        // Verify kit.alias was extracted
        assert_eq!(config.kit.alias.get("$lib"), Some(&"./src/lib".to_string()));
        assert_eq!(
            config.kit.alias.get("$components"),
            Some(&"./src/components".to_string())
        );

        // Verify compilerOptions.runes was extracted
        assert_eq!(config.compiler_options.runes, Some(true));

        // Verify extensions were extracted
        assert!(config.extensions.contains(&".svelte".to_string()));
    }

    #[test]
    fn test_parse_svelte_config_inline() {
        // Test parsing inline config
        let config_content = r#"
            export default {
                kit: {
                    alias: {
                        '$lib': './src/lib',
                        '$utils': './src/utils'
                    }
                },
                compilerOptions: {
                    runes: true,
                    experimental: {
                        async: true
                    }
                }
            };
        "#;

        // Write to temp file and parse
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("svelte.config.js");
        std::fs::write(&config_path, config_content).unwrap();

        let utf8_path = Utf8PathBuf::try_from(temp_dir).unwrap();
        let config = SvelteConfig::load(&utf8_path);

        assert_eq!(config.kit.alias.get("$lib"), Some(&"./src/lib".to_string()));
        assert_eq!(
            config.kit.alias.get("$utils"),
            Some(&"./src/utils".to_string())
        );
        assert_eq!(config.compiler_options.runes, Some(true));
        assert_eq!(config.compiler_options.experimental_async, Some(true));

        // Cleanup
        std::fs::remove_file(config_path).ok();
    }

    #[test]
    fn test_parse_svelte_config_ts() {
        // Issue #3009: a `svelte.config.ts` must be probed and parsed with the
        // TypeScript SWC branch (so TS-only syntax is accepted) and its
        // kit.alias / compilerOptions.runes honored.
        let path = Utf8Path::new("../../test-fixtures/projects/svelte-config-ts");
        let config = SvelteConfig::load(path);

        assert_eq!(config.kit.alias.get("$lib"), Some(&"./src/lib".to_string()));
        assert_eq!(config.compiler_options.runes, Some(true));
    }

    #[test]
    fn test_parse_svelte_config_mts_uses_ts_syntax() {
        // Issue #3009 regression guard: `"svelte.config.mts".ends_with(".ts")`
        // is false, so before the fix `.mts` parsed with the ES branch and
        // would reject TS-only syntax. Use a `satisfies` clause (TS-only) so
        // this test fails if the ES branch is ever (re)selected for `.mts`.
        let config_content = r#"
            type Config = { kit: { alias: Record<string, string> } };
            const config = {
                kit: {
                    alias: {
                        '$lib': './src/lib'
                    }
                }
            } satisfies Config;
            export default config;
        "#;

        let temp_dir = std::env::temp_dir().join("svelte_check_rs_mts_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("svelte.config.mts");
        std::fs::write(&config_path, config_content).unwrap();

        let utf8_path = Utf8PathBuf::try_from(temp_dir.clone()).unwrap();
        let config = SvelteConfig::load(&utf8_path);

        assert_eq!(
            config.kit.alias.get("$lib"),
            Some(&"./src/lib".to_string()),
            "expected .mts config to parse with the TypeScript SWC branch"
        );

        // Cleanup
        std::fs::remove_file(&config_path).ok();
        std::fs::remove_dir(&temp_dir).ok();
    }

    #[test]
    fn test_parse_svelte_config_cjs() {
        // `.cjs` is probed and parsed. This uses a *real* CommonJS body
        // (`module.exports = { ... }`); previously this test wrote ESM `export
        // default` into a `.cjs` file, which masked the gap that the static
        // extractor only understood `export default`. With CommonJS support the
        // static extractor must read `module.exports` and honor the alias.
        let config_content = r#"
            module.exports = {
                kit: {
                    alias: {
                        '$lib': './src/lib'
                    }
                }
            };
        "#;

        let temp_dir = std::env::temp_dir().join("svelte_check_rs_cjs_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("svelte.config.cjs");
        std::fs::write(&config_path, config_content).unwrap();

        let utf8_path = Utf8PathBuf::try_from(temp_dir.clone()).unwrap();
        let config = SvelteConfig::load(&utf8_path);

        assert_eq!(
            config.kit.alias.get("$lib"),
            Some(&"./src/lib".to_string()),
            "expected CommonJS module.exports .cjs config to be parsed"
        );

        // Cleanup
        std::fs::remove_file(&config_path).ok();
        std::fs::remove_dir(&temp_dir).ok();
    }

    #[test]
    fn test_parse_svelte_config_cjs_module_exports_ident() {
        // CommonJS where `module.exports` is assigned an identifier referring to
        // a top-level `const`. The RHS identifier must be resolved via
        // object_by_name, mirroring `export default config`.
        let config_content = r#"
            const config = {
                kit: {
                    alias: {
                        '$lib': './src/lib'
                    }
                },
                compilerOptions: {
                    runes: true
                }
            };
            module.exports = config;
        "#;

        let temp_dir = std::env::temp_dir().join("svelte_check_rs_cjs_ident_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("svelte.config.cjs");
        std::fs::write(&config_path, config_content).unwrap();

        let utf8_path = Utf8PathBuf::try_from(temp_dir.clone()).unwrap();
        let config = SvelteConfig::load(&utf8_path);

        assert_eq!(
            config.kit.alias.get("$lib"),
            Some(&"./src/lib".to_string()),
            "expected `module.exports = config` to resolve the identifier RHS"
        );
        assert_eq!(config.compiler_options.runes, Some(true));

        // Cleanup
        std::fs::remove_file(&config_path).ok();
        std::fs::remove_dir(&temp_dir).ok();
    }

    #[test]
    fn test_parse_svelte_config_exports_bare() {
        // CommonJS bare `exports = { ... }` form (without the `module.` prefix).
        let config_content = r#"
            exports = {
                kit: {
                    alias: {
                        '$lib': './src/lib'
                    }
                }
            };
        "#;

        let temp_dir = std::env::temp_dir().join("svelte_check_rs_exports_bare_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("svelte.config.cjs");
        std::fs::write(&config_path, config_content).unwrap();

        let utf8_path = Utf8PathBuf::try_from(temp_dir.clone()).unwrap();
        let config = SvelteConfig::load(&utf8_path);

        assert_eq!(
            config.kit.alias.get("$lib"),
            Some(&"./src/lib".to_string()),
            "expected bare `exports = {{ ... }}` to be parsed"
        );

        // Cleanup
        std::fs::remove_file(&config_path).ok();
        std::fs::remove_dir(&temp_dir).ok();
    }

    #[test]
    fn test_parse_svelte_config_cjs_fixture() {
        // Static on-disk fixture: a real CommonJS `.cjs` config mirroring the
        // `svelte-config-ts` fixture. Exercises load() -> parse_config() ->
        // module.exports extraction against an actual file rather than a temp.
        let path = Utf8Path::new("../../test-fixtures/projects/svelte-config-cjs");
        let config = SvelteConfig::load(path);

        assert_eq!(
            config.kit.alias.get("$lib"),
            Some(&"./src/lib".to_string()),
            "expected the .cjs fixture's kit.alias.$lib to be parsed"
        );
        assert_eq!(config.compiler_options.runes, Some(true));
    }

    #[test]
    fn test_parse_vite_config_runes_and_alias() {
        // Issue #3031: a plain Svelte+Vite app whose Svelte config lives ONLY in
        // vite.config.ts (no svelte.config.*) must be honored. The static
        // approximation resolves the `svelte({...})` plugin options.
        let path = Utf8Path::new("../../test-fixtures/projects/vite-config-svelte");
        let config = SvelteConfig::load(path);

        assert_eq!(
            config.compiler_options.runes,
            Some(true),
            "expected compilerOptions.runes from vite.config.ts svelte() plugin"
        );
        assert_eq!(
            config.kit.alias.get("$lib"),
            Some(&"./src/lib".to_string()),
            "expected kit.alias.$lib from vite.config.ts svelte() plugin"
        );
        assert!(
            config.extensions.contains(&".svelte".to_string()),
            "expected extensions from vite.config.ts svelte() plugin"
        );
    }

    #[test]
    fn test_parse_vite_config_experimental_async() {
        // Issue #3031: `compilerOptions.experimental.async` declared in the
        // vite.config svelte() plugin must be extracted (it flows through the
        // existing extract_config_from_object path).
        let config_content = r#"
            import { defineConfig } from 'vite';
            import { svelte } from '@sveltejs/vite-plugin-svelte';

            export default defineConfig({
                plugins: [
                    svelte({
                        compilerOptions: {
                            experimental: {
                                async: true
                            }
                        }
                    })
                ]
            });
        "#;

        let temp_dir = std::env::temp_dir().join("svelte_check_rs_vite_async_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("vite.config.ts");
        std::fs::write(&config_path, config_content).unwrap();

        let utf8_path = Utf8PathBuf::try_from(temp_dir.clone()).unwrap();
        let config = SvelteConfig::load(&utf8_path);

        assert_eq!(
            config.compiler_options.experimental_async,
            Some(true),
            "expected experimental.async from vite.config.ts svelte() plugin"
        );

        // Cleanup
        std::fs::remove_file(&config_path).ok();
        std::fs::remove_dir(&temp_dir).ok();
    }

    #[test]
    fn test_parse_vite_config_defineconfig_wrapper() {
        // Issue #3031: the `defineConfig(<obj>)` call wrapper must be unwrapped
        // to its first object argument before scanning `plugins`.
        let config_content = r#"
            import { defineConfig } from 'vite';
            import { sveltekit } from '@sveltejs/kit/vite';

            export default defineConfig({
                plugins: [sveltekit({ compilerOptions: { runes: true } })]
            });
        "#;

        let temp_dir = std::env::temp_dir().join("svelte_check_rs_vite_defineconfig_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("vite.config.ts");
        std::fs::write(&config_path, config_content).unwrap();

        let utf8_path = Utf8PathBuf::try_from(temp_dir.clone()).unwrap();
        let config = SvelteConfig::load(&utf8_path);

        assert_eq!(
            config.compiler_options.runes,
            Some(true),
            "expected runes from sveltekit({{...}}) inside defineConfig(...)"
        );

        // Cleanup
        std::fs::remove_file(&config_path).ok();
        std::fs::remove_dir(&temp_dir).ok();
    }

    #[test]
    fn test_vite_config_precedence_over_svelte_config() {
        // Issue #3031: when BOTH a vite.config (with options) and a svelte.config
        // exist, vite.config wins (all-or-nothing precedence, no merge).
        let path = Utf8Path::new("../../test-fixtures/projects/vite-config-precedence");
        let config = SvelteConfig::load(path);

        assert_eq!(
            config.kit.alias.get("$lib"),
            Some(&"./from-vite".to_string()),
            "expected vite.config.ts alias to win over svelte.config.js"
        );
    }

    #[test]
    fn test_vite_config_bare_sveltekit_falls_through() {
        // Issue #3031 CRITICAL: a bare `sveltekit()` (no options object) yields
        // no plugin options, so parse_vite_config returns Ok(None) and load()
        // MUST fall through to svelte.config.js. This guards the existing
        // sveltekit-bundler/nodenext/svelte-modules fixtures, which all pair a
        // bare `sveltekit()` vite.config with a real svelte.config.js.
        let vite_content = r#"
            import { sveltekit } from '@sveltejs/kit/vite';
            import { defineConfig } from 'vite';

            export default defineConfig({
                plugins: [sveltekit()]
            });
        "#;
        let svelte_content = r#"
            export default {
                kit: {
                    alias: {
                        '$lib': './src/lib'
                    }
                }
            };
        "#;

        let temp_dir = std::env::temp_dir().join("svelte_check_rs_vite_bare_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let vite_path = temp_dir.join("vite.config.ts");
        let svelte_path = temp_dir.join("svelte.config.js");
        std::fs::write(&vite_path, vite_content).unwrap();
        std::fs::write(&svelte_path, svelte_content).unwrap();

        let utf8_path = Utf8PathBuf::try_from(temp_dir.clone()).unwrap();
        let config = SvelteConfig::load(&utf8_path);

        assert_eq!(
            config.kit.alias.get("$lib"),
            Some(&"./src/lib".to_string()),
            "expected fall-through to svelte.config.js when vite has only bare sveltekit()"
        );

        // Cleanup
        std::fs::remove_file(&vite_path).ok();
        std::fs::remove_file(&svelte_path).ok();
        std::fs::remove_dir(&temp_dir).ok();
    }

    #[test]
    fn test_requires_explicit_extensions() {
        // NodeNext requires explicit extensions
        let opts = CompilerOptions {
            module: Some("NodeNext".to_string()),
            ..Default::default()
        };
        assert!(opts.requires_explicit_extensions());

        // Node16 requires explicit extensions
        let opts = CompilerOptions {
            module: Some("Node16".to_string()),
            ..Default::default()
        };
        assert!(opts.requires_explicit_extensions());

        // Case insensitive
        let opts = CompilerOptions {
            module: Some("nodenext".to_string()),
            ..Default::default()
        };
        assert!(opts.requires_explicit_extensions());

        // moduleResolution takes precedence
        let opts = CompilerOptions {
            module: Some("ESNext".to_string()),
            module_resolution: Some("NodeNext".to_string()),
            ..Default::default()
        };
        assert!(opts.requires_explicit_extensions());

        // Bundler does not require explicit extensions
        let opts = CompilerOptions {
            module: Some("ESNext".to_string()),
            module_resolution: Some("bundler".to_string()),
            ..Default::default()
        };
        assert!(!opts.requires_explicit_extensions());

        // Default does not require explicit extensions
        let opts = CompilerOptions::default();
        assert!(!opts.requires_explicit_extensions());
    }

    #[test]
    fn test_file_extensions_defaults_when_unset() {
        let config = SvelteConfig::default();
        assert_eq!(
            config.file_extensions(),
            vec![".svelte.ts", ".svelte.js", ".svelte"]
        );
        assert!(config.unsupported_extensions().is_empty());
    }

    #[test]
    fn test_file_extensions_merges_with_user_extensions() {
        // Issue #126: when svelte.config.js declares extensions like `.svx`
        // from mdsvex, we still need to discover the natively-supported ones
        // (`.svelte`, `.svelte.ts`, `.svelte.js`) AND the user's extras.
        let config = SvelteConfig {
            extensions: vec![".svelte".to_string(), ".svx".to_string()],
            ..Default::default()
        };
        let extensions = config.file_extensions();
        assert!(extensions.contains(&".svelte"));
        assert!(extensions.contains(&".svelte.ts"));
        assert!(extensions.contains(&".svelte.js"));
        assert!(extensions.contains(&".svx"));
        // `.svelte` listed twice (once natively, once by user) must dedupe.
        assert_eq!(extensions.iter().filter(|e| **e == ".svelte").count(), 1);
    }

    #[test]
    fn test_unsupported_extensions_excludes_natives() {
        let config = SvelteConfig {
            extensions: vec![
                ".svelte".to_string(),
                ".svelte.ts".to_string(),
                ".svx".to_string(),
                ".mdx".to_string(),
            ],
            ..Default::default()
        };
        let mut unsupported = config.unsupported_extensions();
        unsupported.sort();
        assert_eq!(unsupported, vec![".mdx", ".svx"]);
    }
}
