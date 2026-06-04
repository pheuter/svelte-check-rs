use camino::Utf8Path;
use std::sync::Arc;
use swc_common::{FileName, SourceMap, Span, Spanned};
use swc_ecma_ast::{
    ArrayLit, BlockStmtOrExpr, CallExpr, Callee, CondExpr, Decl, ExportDecl, Expr, ExprOrSpread,
    FnDecl, Function, MemberProp, Module, ModuleDecl, ModuleItem, Pat, VarDecl, VarDeclarator,
};
use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax};
use swc_ecma_visit::{Visit, VisitWith};

#[derive(Debug, Clone, Copy)]
pub(crate) enum KitFileKind {
    Route(KitRouteKind),
    ServerHooks,
    ClientHooks,
    UniversalHooks,
    Params,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct KitRouteKind {
    pub is_layout: bool,
    pub is_server: bool,
    pub is_endpoint: bool,
}

/// Accepts every JavaScript/TypeScript extension SvelteKit recognizes for
/// hooks, params, and route scripts.  Node's `--experimental-loader=ts-node`
/// and packages with `"type": "module"` mean `.mts`/`.cts`/`.mjs`/`.cjs` are
/// not theoretical — projects ship them.
pub(crate) fn is_kit_script_ext(ext: &str) -> bool {
    matches!(ext, "ts" | "js" | "mts" | "cts" | "mjs" | "cjs")
}

const KIT_SCRIPT_EXTS: &[&str] = &["ts", "js", "mts", "cts", "mjs", "cjs"];

fn matches_kit_script_suffix(rel_str: &str, base: &str) -> bool {
    KIT_SCRIPT_EXTS
        .iter()
        .any(|ext| rel_str.ends_with(&format!("{base}.{ext}")))
}

pub(crate) fn kit_file_kind(path: &Utf8Path, project_root: &Utf8Path) -> Option<KitFileKind> {
    let ext = path.extension()?;
    if !is_kit_script_ext(ext) {
        return None;
    }

    let file_name = path.file_name()?;
    if let Some(route_kind) = kit_route_kind(file_name) {
        return Some(KitFileKind::Route(route_kind));
    }

    let rel = path.strip_prefix(project_root).ok().unwrap_or(path);
    // Normalize to forward slashes so the substring checks below work on
    // Windows, where `Utf8Path::as_str()` returns backslash-separated paths.
    let rel_str = rel.as_str().replace('\\', "/");
    let rel_str = rel_str.trim_start_matches('/');

    if matches_kit_script_suffix(rel_str, "src/hooks.server") {
        return Some(KitFileKind::ServerHooks);
    }
    if matches_kit_script_suffix(rel_str, "src/hooks.client") {
        return Some(KitFileKind::ClientHooks);
    }
    if matches_kit_script_suffix(rel_str, "src/hooks") {
        return Some(KitFileKind::UniversalHooks);
    }

    // SvelteKit's `params/` directory lives directly under the project root's
    // `src/`. Use `starts_with` so a vendored library at
    // `src/lib/vendored/pkg/src/params/match.ts` isn't misclassified and
    // injected with `ParamMatcher` augmentations it doesn't expect.
    if rel_str.starts_with("src/params/")
        && !file_name.contains(".test")
        && !file_name.contains(".spec")
    {
        return Some(KitFileKind::Params);
    }

    None
}

pub(crate) fn transform_kit_source(
    kind: KitFileKind,
    path: &Utf8Path,
    source: &str,
) -> Option<String> {
    // TS-flavoured extensions need the TS parser even when the suffix is
    // `.mts`/`.cts`; JS-flavoured extensions parse with the ES parser.
    let is_ts = matches!(path.extension(), Some("ts" | "tsx" | "mts" | "cts"));
    let module = parse_module(path, source, is_ts)?;
    let mut insertions: Vec<Insertion> = Vec::new();

    match kind {
        KitFileKind::Route(route_kind) => {
            apply_route_transforms(&module, source, is_ts, route_kind, &mut insertions);
        }
        KitFileKind::ServerHooks => {
            apply_hooks_transforms(
                &module,
                source,
                is_ts,
                &["handleError", "handle", "handleFetch"],
                "import('@sveltejs/kit').HandleServerError",
                "import('@sveltejs/kit').Handle",
                "import('@sveltejs/kit').HandleFetch",
                &mut insertions,
            );
        }
        KitFileKind::ClientHooks => {
            apply_hooks_transforms(
                &module,
                source,
                is_ts,
                &["handleError"],
                "import('@sveltejs/kit').HandleClientError",
                "",
                "",
                &mut insertions,
            );
        }
        KitFileKind::UniversalHooks => {
            apply_hooks_transforms(
                &module,
                source,
                is_ts,
                &["reroute"],
                "import('@sveltejs/kit').Reroute",
                "",
                "",
                &mut insertions,
            );
        }
        KitFileKind::Params => {
            apply_params_transforms(&module, source, is_ts, &mut insertions);
        }
    }

    // Handle Promise.all with conditional empty arrays (tsgo inference workaround)
    find_promise_all_empty_arrays(&module, source, &mut insertions);

    if insertions.is_empty() {
        return None;
    }

    Some(apply_insertions(source, insertions))
}

fn parse_module(path: &Utf8Path, source: &str, is_ts: bool) -> Option<Module> {
    let cm: Arc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom(path.to_string()).into(),
        source.to_string(),
    );
    let syntax = if is_ts {
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
    let mut parser = Parser::new(syntax, StringInput::from(&*fm), None);
    parser.parse_module().ok()
}

fn kit_route_kind(file_name: &str) -> Option<KitRouteKind> {
    let name = match file_name.rsplit_once('.') {
        Some((stem, _ext)) => stem,
        None => file_name,
    };
    let name = if let Some((base, _)) = name.split_once('@') {
        base
    } else {
        name
    };

    let (is_layout, is_server, is_endpoint) = match name {
        "+page" => (false, false, false),
        "+layout" => (true, false, false),
        "+page.server" => (false, true, false),
        "+layout.server" => (true, true, false),
        "+server" => (false, true, true),
        _ => return None,
    };

    Some(KitRouteKind {
        is_layout,
        is_server,
        is_endpoint,
    })
}

fn apply_route_transforms(
    module: &Module,
    source: &str,
    is_ts: bool,
    kind: KitRouteKind,
    insertions: &mut Vec<Insertion>,
) {
    let base = if kind.is_layout { "Layout" } else { "Page" };
    let server_suffix = if kind.is_server { "Server" } else { "" };
    let load_event = format!("{base}{server_suffix}LoadEvent");
    let load_type = format!("{base}{server_suffix}Load");
    let actions_type = "Actions";
    let entry_type = "EntryGenerator";
    let request_event = "RequestEvent";

    let http_methods = [
        "GET", "PUT", "POST", "PATCH", "DELETE", "OPTIONS", "HEAD", "fallback",
    ];

    for item in &module.body {
        if let Some(exports) = export_decl(item, source) {
            for export in exports {
                match export {
                    ExportDeclKind::Fn(name, func, export_start) => {
                        if name == "load" && !kind.is_endpoint {
                            add_param_type_if_missing(
                                func,
                                source,
                                is_ts,
                                export_start,
                                &format!("import('./$types.js').{load_event}"),
                                insertions,
                            );
                        } else if name == "entries" && !kind.is_layout {
                            // entries: `.ts` gets `: ReturnType<EntryGenerator>`;
                            // `.js` gets a bare JSDoc `@type {EntryGenerator}`.
                            add_return_type_if_missing(
                                func,
                                source,
                                is_ts,
                                export_start,
                                &format!("ReturnType<import('./$types.js').{entry_type}>"),
                                &format!("import('./$types.js').{entry_type}"),
                                insertions,
                            );
                        } else if http_methods.contains(&name.as_str()) {
                            // HTTP methods: `.ts` keeps the param-only annotation
                            // (no forced return type — preserves the user's
                            // freedom); `.js` emits the full callable JSDoc
                            // `@type {(arg0: RequestEvent) => Response | Promise<Response>}`.
                            add_http_method_if_missing(
                                func,
                                source,
                                is_ts,
                                export_start,
                                &format!("import('./$types.js').{request_event}"),
                                "Response | Promise<Response>",
                                insertions,
                            );
                        }
                    }
                    ExportDeclKind::Var(name, decl, export_start) => {
                        if name == "load" && !kind.is_endpoint {
                            if !pat_has_type_ann(&decl.name) {
                                if let Some(init) = &decl.init {
                                    if expr_contains_satisfies(source, init.span()) {
                                        continue;
                                    }
                                    // JS-only: a leading JSDoc `@satisfies` tag
                                    // is the comment form of `(...) satisfies T`.
                                    // swc strips comments, so it never lands in
                                    // `init.span()` — guard explicitly, mirroring
                                    // upstream's `!isTsFile && getJSDocTags`
                                    // `@satisfies` clause in `hasTypeDefinition`.
                                    // The comment sits before the `export`
                                    // keyword, so scan from `export_start`.
                                    if !is_ts && has_leading_jsdoc_satisfies(source, export_start) {
                                        continue;
                                    }
                                    let load_type = format!("import('./$types.js').{load_type}");
                                    if is_ts {
                                        let start =
                                            expr_start_with_async(source, span_lo(init.span()));
                                        push_insertion(insertions, start, "(".to_string());
                                        let end =
                                            expr_end_before_semi(source, span_hi(init.span()));
                                        push_insertion(
                                            insertions,
                                            end,
                                            format!(") satisfies {load_type}"),
                                        );
                                    } else if jsdoc_decl_present(source, decl, export_start) {
                                        // User already typed the load via JSDoc.
                                    } else if let Some(func_like) =
                                        function_like_from_paren(init, export_start)
                                    {
                                        // Upstream classifies a function-like
                                        // load initializer as `type:'function'`
                                        // and emits a `@param` JSDoc at the
                                        // arrow/fn start, NOT a `@satisfies`
                                        // wrap.
                                        let load_event =
                                            format!("import('./$types.js').{load_event}");
                                        add_jsdoc_param_to_function_fnlike(
                                            func_like,
                                            source,
                                            &load_event,
                                            insertions,
                                        );
                                    } else {
                                        // Non-function load initializer →
                                        // `@satisfies` wrap of the value.
                                        add_jsdoc_satisfies_to_variable(
                                            source, decl, &load_type, insertions,
                                        );
                                    }
                                }
                            }
                        } else if name == "actions" {
                            if !pat_has_type_ann(&decl.name) {
                                if let Some(init) = &decl.init {
                                    if expr_contains_satisfies(source, init.span()) {
                                        continue;
                                    }
                                    // JS-only: see the `load` branch — a leading
                                    // JSDoc `@satisfies` already satisfies the
                                    // type contract, so skip re-injection.  The
                                    // comment precedes `export`, so scan from
                                    // `export_start`.
                                    if !is_ts && has_leading_jsdoc_satisfies(source, export_start) {
                                        continue;
                                    }
                                    let actions_type =
                                        format!("import('./$types.js').{actions_type}");
                                    if is_ts {
                                        push_insertion(
                                            insertions,
                                            expr_end_before_semi(source, span_hi(init.span())),
                                            format!(" satisfies {actions_type}"),
                                        );
                                    } else if !jsdoc_decl_present(source, decl, export_start) {
                                        add_jsdoc_satisfies_to_variable(
                                            source,
                                            decl,
                                            &actions_type,
                                            insertions,
                                        );
                                    }
                                }
                            }
                        } else if matches!(
                            name.as_str(),
                            "prerender" | "trailingSlash" | "ssr" | "csr"
                        ) {
                            if !pat_has_type_ann(&decl.name) {
                                let ty = match name.as_str() {
                                    "prerender" => "boolean | 'auto'",
                                    "trailingSlash" => "'never' | 'always' | 'ignore'",
                                    "ssr" | "csr" => "boolean",
                                    _ => "boolean",
                                };
                                if is_ts {
                                    if let Some(end) = pat_end(&decl.name) {
                                        push_insertion(insertions, end, format!(": {ty}"));
                                    }
                                } else if !jsdoc_decl_present(source, decl, export_start) {
                                    add_jsdoc_type_to_variable(source, decl, ty, insertions);
                                }
                            }
                        } else if name == "entries" && !kind.is_layout {
                            if let Some(func_like) =
                                function_like_from_expr(&decl.init, export_start)
                            {
                                add_return_type_if_missing_fnlike(
                                    func_like,
                                    source,
                                    is_ts,
                                    &format!("ReturnType<import('./$types.js').{entry_type}>"),
                                    &format!("import('./$types.js').{entry_type}"),
                                    insertions,
                                );
                            }
                        } else if http_methods.contains(&name.as_str()) {
                            // If the user already annotated the outer variable
                            // (e.g. `export const GET: RequestHandler = ...`),
                            // leave the destructured parameter alone — adding an
                            // inner `: RequestEvent` annotation would silently
                            // override their explicit `RequestHandler` typing and
                            // mask real `params.X is string | undefined` errors
                            // that the broader `@sveltejs/kit` `RequestHandler`
                            // intentionally surfaces.
                            if !pat_has_type_ann(&decl.name) {
                                if let Some(func_like) =
                                    function_like_from_expr(&decl.init, export_start)
                                {
                                    add_http_method_if_missing_fnlike(
                                        func_like,
                                        source,
                                        is_ts,
                                        &format!("import('./$types.js').{request_event}"),
                                        "Response | Promise<Response>",
                                        insertions,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_hooks_transforms(
    module: &Module,
    source: &str,
    is_ts: bool,
    names: &[&str],
    handle_error_type: &str,
    handle_type: &str,
    handle_fetch_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    // Resolve the raw handler type for an export name.  `addTypeToFunction`
    // expands it to `Parameters<H>[0]`/`ReturnType<H>` on the TS path and
    // emits the bare `H` as a JSDoc `@type` on the JS path — mirroring
    // upstream, where the JSDoc branch uses the unexpanded handler type.
    let handler_type = |name: &str| -> Option<&str> {
        match name {
            "handleError" if names.contains(&"handleError") => Some(handle_error_type),
            "handle" if names.contains(&"handle") => Some(handle_type),
            "handleFetch" if names.contains(&"handleFetch") => Some(handle_fetch_type),
            // Universal `reroute` reuses the first type slot (`handle_error_type`).
            "reroute" if names.contains(&"reroute") => Some(handle_error_type),
            _ => None,
        }
    };

    for item in &module.body {
        if let Some(exports) = export_decl(item, source) {
            for export in exports {
                match export {
                    ExportDeclKind::Fn(name, func, export_start) => {
                        if let Some(raw_type) = handler_type(&name) {
                            add_handler_if_missing(
                                func,
                                source,
                                is_ts,
                                export_start,
                                raw_type,
                                insertions,
                            );
                        }
                    }
                    ExportDeclKind::Var(name, decl, export_start) => {
                        if let Some(func_like) = function_like_from_expr(&decl.init, export_start) {
                            if let Some(raw_type) = handler_type(&name) {
                                add_handler_if_missing_fnlike(
                                    func_like, source, is_ts, raw_type, insertions,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn apply_params_transforms(
    module: &Module,
    source: &str,
    is_ts: bool,
    insertions: &mut Vec<Insertion>,
) {
    let mut found_match = false;
    for item in &module.body {
        if let Some(exports) = export_decl(item, source) {
            for export in exports {
                match export {
                    ExportDeclKind::Fn(name, func, export_start) => {
                        if name == "match" {
                            // Only constrain the `param` argument's type. Leaving the
                            // return type untouched preserves TypeScript 5.5+ inferred
                            // type predicates (e.g. `(p: string) => p === 'a' || p === 'b'`
                            // is treated as `(p: string) => p is 'a' | 'b'`), which
                            // SvelteKit's generated `MatcherParam<typeof match>` relies
                            // on to narrow route params.  The `ParamMatcher` constraint
                            // is enforced separately via a trailing `satisfies` check
                            // appended below.  On `.js` this becomes a JSDoc
                            // `@param {string}` (not upstream's
                            // `@type {(arg0: string) => boolean}`, which would
                            // force a `boolean` return and defeat the predicate).
                            add_param_type_if_missing(
                                func,
                                source,
                                is_ts,
                                export_start,
                                "string",
                                insertions,
                            );
                            found_match = true;
                        }
                    }
                    ExportDeclKind::Var(name, decl, export_start) => {
                        if name == "match" {
                            if let Some(func_like) =
                                function_like_from_expr(&decl.init, export_start)
                            {
                                add_param_type_if_missing_fnlike(
                                    func_like, source, is_ts, "string", insertions,
                                );
                            }
                            found_match = true;
                        }
                    }
                }
            }
        }
    }

    if found_match {
        // Enforce the `ParamMatcher` contract via a trailing reference.  The
        // form is gated on `is_ts`: `.ts` uses the bare `satisfies` operator,
        // but a checked `.js` file (allowJs+checkJs) must NOT receive a TS-only
        // operator or tsgo reports a false-positive TS8010/TS8037 — contrary
        // to #2939's "no TS-only syntax in checked .js files" goal.  The `.js`
        // path uses the JSDoc `@satisfies` cast instead, which preserves the
        // inferred type predicate identically (see const docs below).
        let suffix = if is_ts {
            PARAM_MATCHER_SATISFIES_TS
        } else {
            PARAM_MATCHER_SATISFIES_JS
        };
        push_insertion(insertions, source.len(), suffix.to_string());
    }
}

/// Appended to `.ts`/`.mts`/`.cts` params files that export a `match` to
/// enforce the `ParamMatcher` contract.  The `satisfies` operator type-checks
/// the function shape (`(param: string) => boolean`) without widening the
/// inferred type — so type predicates inferred from boolean expressions
/// (TS 5.5+) survive and SvelteKit's `MatcherParam<typeof match>` can extract
/// the narrowed param union.
const PARAM_MATCHER_SATISFIES_TS: &str =
    "\n;void (match satisfies import('@sveltejs/kit').ParamMatcher);\n";

/// The `.js`/`.mjs`/`.cjs` counterpart of [`PARAM_MATCHER_SATISFIES_TS`].  The
/// bare `satisfies` operator is TS-only syntax: in a checked `.js` file
/// (allowJs+checkJs) it triggers a false-positive TS8010/TS8037, contradicting
/// #2939's goal of emitting no TS-only syntax into checked `.js` files.  The
/// JSDoc `@satisfies` cast (`/** @satisfies {T} */ (match)`) is the
/// JS-compatible equivalent — it enforces the same `ParamMatcher` shape and,
/// crucially, preserves the TS 5.5+ inferred type predicate exactly like the
/// operator form (it does not widen).  This is the same construct emitted by
/// [`add_jsdoc_satisfies_to_variable`] and validated against the real tsgo
/// pipeline by the #2946 integration tests.
const PARAM_MATCHER_SATISFIES_JS: &str =
    "\n;void (/** @satisfies {import('@sveltejs/kit').ParamMatcher} */ (match));\n";

enum ExportDeclKind<'a> {
    /// `export function NAME(...) {}`.  `export_start` is the 0-indexed byte
    /// offset of the enclosing `export` keyword — JSDoc comments for the
    /// `.js` path are inserted there to mirror upstream's `node.getStart()`,
    /// which for a `FunctionDeclaration` includes the leading `export`
    /// modifier.
    Fn(String, &'a Function, usize),
    /// `export const NAME = ...`.  The trailing `usize` is the 0-indexed byte
    /// offset of the enclosing `export` keyword (anchor for the
    /// JSDoc-already-present guard on the `.js` path).
    Var(String, &'a VarDeclarator, usize),
}

fn export_decl<'a>(item: &'a ModuleItem, source: &str) -> Option<Vec<ExportDeclKind<'a>>> {
    match item {
        ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl { decl, span })) => {
            // swc's `ExportDecl` span includes any leading JSDoc/line
            // comments, so resolve the real `export` keyword offset.  This is
            // upstream's `node.getStart()` (which excludes leading trivia) and
            // is both the JSDoc insertion point and the anchor for the
            // JSDoc-already-present guard.
            let export_start = find_export_keyword(source, span_lo(*span));
            export_from_decl(decl, export_start)
        }
        _ => None,
    }
}

/// Resolves the byte offset of the `export` keyword starting from a span lo
/// that may point at leading trivia (comments/whitespace).  Falls back to the
/// given offset if `export` isn't found in the immediate window.
fn find_export_keyword(source: &str, span_lo: usize) -> usize {
    match source.get(span_lo..) {
        Some(rest) => rest.find("export").map(|i| span_lo + i).unwrap_or(span_lo),
        None => span_lo,
    }
}

fn export_from_decl(decl: &Decl, export_start: usize) -> Option<Vec<ExportDeclKind<'_>>> {
    match decl {
        Decl::Fn(FnDecl {
            ident, function, ..
        }) => Some(vec![ExportDeclKind::Fn(
            ident.sym.to_string(),
            function,
            export_start,
        )]),
        Decl::Var(var) => export_from_var_decl(var, export_start),
        _ => None,
    }
}

fn export_from_var_decl(decl: &VarDecl, export_start: usize) -> Option<Vec<ExportDeclKind<'_>>> {
    let exports: Vec<_> = decl
        .decls
        .iter()
        .filter_map(|d| {
            if let Pat::Ident(ident) = &d.name {
                Some(ExportDeclKind::Var(
                    ident.id.sym.to_string(),
                    d,
                    export_start,
                ))
            } else {
                None
            }
        })
        .collect();
    if exports.is_empty() {
        None
    } else {
        Some(exports)
    }
}

fn add_param_type_if_missing(
    func: &Function,
    source: &str,
    is_ts: bool,
    export_start: usize,
    type_expr: &str,
    insertions: &mut Vec<Insertion>,
) {
    if func.params.len() != 1 {
        return;
    }
    let param = &func.params[0].pat;
    if pat_has_type_ann(param) {
        return;
    }
    if is_ts {
        if let Some(pos) = pat_end(param) {
            let insert_pos = adjust_param_insert_pos(source, pos);
            push_insertion(insertions, insert_pos, format!(": {type_expr}"));
        }
    } else {
        // JS: `/** @param {T} <name> */ ` before `export`.  Skip if the
        // user already supplied a `@type`/`@param` JSDoc.
        if jsdoc_type_present_before(source, export_start) {
            return;
        }
        let name = pat_ident_name(param).unwrap_or("arg0");
        push_insertion(
            insertions,
            export_start,
            format!("/** @param {{{type_expr}}} {name} */ "),
        );
    }
}

/// entries: `: ReturnType<EntryGenerator> ` (TS) vs a bare JSDoc
/// `@type {EntryGenerator}` (JS).  `ts_return_type` is the fully expanded
/// return type for the TS branch; `js_type` is the unexpanded type used by
/// the JSDoc branch.
#[allow(clippy::too_many_arguments)]
fn add_return_type_if_missing(
    func: &Function,
    source: &str,
    is_ts: bool,
    export_start: usize,
    ts_return_type: &str,
    js_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    if func.return_type.is_some() {
        return;
    }
    if is_ts {
        if let Some(body) = &func.body {
            let adjusted_type = adjust_return_type_for_async(ts_return_type, func.is_async);
            let insert_pos = adjust_return_type_insert_pos(source, span_lo(body.span()));
            push_insertion(insertions, insert_pos, format!(": {adjusted_type} "));
        }
    } else {
        if jsdoc_type_present_before(source, export_start) {
            return;
        }
        push_insertion(
            insertions,
            export_start,
            format!("/** @type {{{js_type}}} */ "),
        );
    }
}

/// HTTP methods (GET/PUT/...).  TS keeps the existing param-only annotation
/// (no forced return type); JS emits the full callable JSDoc
/// `@type {(arg0: <param_type>) => <return_type>}`.
fn add_http_method_if_missing(
    func: &Function,
    source: &str,
    is_ts: bool,
    export_start: usize,
    param_type: &str,
    return_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    if func.params.len() != 1 {
        return;
    }
    let param = &func.params[0].pat;
    if pat_has_type_ann(param) {
        return;
    }
    if is_ts {
        if let Some(pos) = pat_end(param) {
            let insert_pos = adjust_param_insert_pos(source, pos);
            push_insertion(insertions, insert_pos, format!(": {param_type}"));
        }
    } else {
        if jsdoc_type_present_before(source, export_start) {
            return;
        }
        push_insertion(
            insertions,
            export_start,
            format!("/** @type {{(arg0: {param_type}) => {return_type}}} */ "),
        );
    }
}

/// Hooks (`handle`/`handleError`/`handleFetch`/`reroute`).  `raw_type` is the
/// unexpanded handler type (e.g. `import('@sveltejs/kit').Handle`).  TS
/// expands it to `Parameters<raw>[0]` / `ReturnType<raw>`; JS emits a bare
/// JSDoc `@type {raw}`.
fn add_handler_if_missing(
    func: &Function,
    source: &str,
    is_ts: bool,
    export_start: usize,
    raw_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    if is_ts {
        // Mirror the original `add_param_and_return_if_missing`: param and
        // return insertions are independent (return is added whenever the
        // function has no explicit return type and a body, regardless of the
        // param check).
        add_param_type_if_missing(
            func,
            source,
            true,
            export_start,
            &format!("Parameters<{raw_type}>[0]"),
            insertions,
        );
        if func.return_type.is_none() {
            if let Some(body) = &func.body {
                let adjusted_type =
                    adjust_return_type_for_async(&format!("ReturnType<{raw_type}>"), func.is_async);
                let insert_pos = adjust_return_type_insert_pos(source, span_lo(body.span()));
                push_insertion(insertions, insert_pos, format!(": {adjusted_type} "));
            }
        }
    } else {
        if func.params.len() != 1 || pat_has_type_ann(&func.params[0].pat) {
            return;
        }
        if jsdoc_type_present_before(source, export_start) {
            return;
        }
        push_insertion(
            insertions,
            export_start,
            format!("/** @type {{{raw_type}}} */ "),
        );
    }
}

#[derive(Clone, Copy)]
enum FunctionLikeParams<'a> {
    Arrow(&'a [Pat]),
    Fn(&'a [swc_ecma_ast::Param]),
}

impl<'a> FunctionLikeParams<'a> {
    fn len(&self) -> usize {
        match self {
            FunctionLikeParams::Arrow(params) => params.len(),
            FunctionLikeParams::Fn(params) => params.len(),
        }
    }

    fn first_pat(&self) -> Option<&'a Pat> {
        match self {
            FunctionLikeParams::Arrow(params) => params.first(),
            FunctionLikeParams::Fn(params) => params.first().map(|param| &param.pat),
        }
    }
}

#[derive(Clone, Copy)]
struct FunctionLike<'a> {
    params: FunctionLikeParams<'a>,
    return_type: Option<&'a swc_ecma_ast::TsTypeAnn>,
    body_start: usize,
    /// 0-indexed byte offset of the arrow/function-expression start (the
    /// `async`/`function`/`(` token).  Mirrors upstream's `node.getStart()`
    /// for the var function-like JSDoc insertion point.
    node_start: usize,
    /// 0-indexed byte offset of the enclosing `export` keyword.  Used by the
    /// `.js` JSDoc-already-present guard to detect a `@type`/`@param` attached
    /// to the variable statement (which sits before `export`).
    export_start: usize,
    is_arrow: bool,
    is_async: bool,
}

fn function_like_from_expr(
    expr: &Option<Box<swc_ecma_ast::Expr>>,
    export_start: usize,
) -> Option<FunctionLike<'_>> {
    function_like_from_inner(expr.as_deref()?, export_start)
}

/// Like [`function_like_from_expr`] but unwraps a single layer of
/// parentheses, e.g. `(async (e) => {})`.  swc's arrow span starts at the
/// `async`/`(` token *inside* the user's parens, matching the upstream
/// `node.getStart()` insertion point for the load-const JSDoc case.
fn function_like_from_paren(
    init: &swc_ecma_ast::Expr,
    export_start: usize,
) -> Option<FunctionLike<'_>> {
    match init {
        swc_ecma_ast::Expr::Paren(paren) => {
            function_like_from_inner(paren.expr.as_ref(), export_start)
        }
        other => function_like_from_inner(other, export_start),
    }
}

fn function_like_from_inner(
    expr: &swc_ecma_ast::Expr,
    export_start: usize,
) -> Option<FunctionLike<'_>> {
    match expr {
        swc_ecma_ast::Expr::Arrow(arrow) => {
            let body_start = match arrow.body.as_ref() {
                BlockStmtOrExpr::BlockStmt(block) => span_lo(block.span()),
                BlockStmtOrExpr::Expr(expr) => span_lo(expr.span()),
            };
            Some(FunctionLike {
                params: FunctionLikeParams::Arrow(&arrow.params),
                return_type: arrow.return_type.as_deref(),
                body_start,
                node_start: span_lo(arrow.span()),
                export_start,
                is_arrow: true,
                is_async: arrow.is_async,
            })
        }
        swc_ecma_ast::Expr::Fn(func) => {
            let body_start = span_lo(func.function.body.as_ref()?.span());
            Some(FunctionLike {
                params: FunctionLikeParams::Fn(&func.function.params),
                return_type: func.function.return_type.as_deref(),
                body_start,
                node_start: span_lo(func.function.span()),
                export_start,
                is_arrow: false,
                is_async: func.function.is_async,
            })
        }
        _ => None,
    }
}

fn add_param_type_if_missing_fnlike(
    func: FunctionLike<'_>,
    source: &str,
    is_ts: bool,
    param_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    if func.params.len() != 1 {
        return;
    }
    let param = match func.params.first_pat() {
        Some(param) => param,
        None => return,
    };
    if pat_has_type_ann(param) {
        return;
    }
    if is_ts {
        if let Some(pos) = pat_end(param) {
            let insert_pos = adjust_param_insert_pos(source, pos);
            push_insertion(insertions, insert_pos, format!(": {param_type}"));
        }
    } else {
        add_jsdoc_param_to_function_fnlike(func, source, param_type, insertions);
    }
}

/// `.js`-only: emits `/** @param {T} <name> */ ` at the arrow/fn-expr start
/// (mirrors upstream's `addJsDocParamToFunction` on the var function-like
/// path).
fn add_jsdoc_param_to_function_fnlike(
    func: FunctionLike<'_>,
    source: &str,
    param_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    let param = match func.params.first_pat() {
        Some(param) => param,
        None => return,
    };
    if pat_has_type_ann(param) {
        return;
    }
    if fnlike_jsdoc_present(source, func) {
        return;
    }
    let name = pat_ident_name(param).unwrap_or("arg0");
    push_insertion(
        insertions,
        func.node_start,
        format!("/** @param {{{param_type}}} {name} */ "),
    );
}

/// `.js` JSDoc-already-present guard for a var function-like.  Checks both the
/// position just before the arrow/fn-expr (`= /** @type */ async`) and the
/// position before the enclosing `export` keyword (a JSDoc attached to the
/// whole variable statement).
fn fnlike_jsdoc_present(source: &str, func: FunctionLike<'_>) -> bool {
    jsdoc_type_present_before(source, func.node_start)
        || jsdoc_type_present_before(source, func.export_start)
}

/// `.js` JSDoc-already-present guard for the variable-wrap cases
/// (`@type`/`@satisfies` on the whole initializer).  Checks both before the
/// initializer and before the enclosing `export` keyword.
fn jsdoc_decl_present(source: &str, decl: &VarDeclarator, export_start: usize) -> bool {
    if jsdoc_type_present_before(source, export_start) {
        return true;
    }
    if let Some(init) = &decl.init {
        return jsdoc_type_present_before(source, span_lo(init.span()));
    }
    false
}

fn add_return_type_if_missing_fnlike(
    func: FunctionLike<'_>,
    source: &str,
    is_ts: bool,
    ts_return_type: &str,
    js_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    if func.return_type.is_some() {
        return;
    }
    if is_ts {
        let adjusted_type = adjust_return_type_for_async(ts_return_type, func.is_async);
        let body_start = func.body_start;
        let param_end = func
            .params
            .first_pat()
            .and_then(pat_end)
            .unwrap_or(body_start);

        if func.is_arrow {
            if let Some(arrow_pos) = find_arrow_pos(source, param_end, body_start) {
                push_insertion(insertions, arrow_pos, format!(": {adjusted_type} "));
            }
        } else {
            let insert_pos = adjust_return_type_insert_pos(source, body_start);
            push_insertion(insertions, insert_pos, format!(": {adjusted_type} "));
        }
    } else {
        if fnlike_jsdoc_present(source, func) {
            return;
        }
        push_insertion(
            insertions,
            func.node_start,
            format!("/** @type {{{js_type}}} */ "),
        );
    }
}

/// HTTP method on a `export const NAME = fn` form.  TS: param-only;
/// JS: full callable `@type {(arg0: P) => R}` at the arrow/fn-expr start.
fn add_http_method_if_missing_fnlike(
    func: FunctionLike<'_>,
    source: &str,
    is_ts: bool,
    param_type: &str,
    return_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    if func.params.len() != 1 {
        return;
    }
    let param = match func.params.first_pat() {
        Some(param) => param,
        None => return,
    };
    if pat_has_type_ann(param) {
        return;
    }
    if is_ts {
        if let Some(pos) = pat_end(param) {
            let insert_pos = adjust_param_insert_pos(source, pos);
            push_insertion(insertions, insert_pos, format!(": {param_type}"));
        }
    } else {
        if fnlike_jsdoc_present(source, func) {
            return;
        }
        push_insertion(
            insertions,
            func.node_start,
            format!("/** @type {{(arg0: {param_type}) => {return_type}}} */ "),
        );
    }
}

/// Hooks on a `export const NAME = fn` form.  TS expands `raw_type` to
/// `Parameters<raw>[0]` / `ReturnType<raw>`; JS emits a bare JSDoc
/// `@type {raw}` at the arrow/fn-expr start.
fn add_handler_if_missing_fnlike(
    func: FunctionLike<'_>,
    source: &str,
    is_ts: bool,
    raw_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    if is_ts {
        add_param_type_if_missing_fnlike(
            func,
            source,
            true,
            &format!("Parameters<{raw_type}>[0]"),
            insertions,
        );
        add_return_type_if_missing_fnlike(
            func,
            source,
            true,
            &format!("ReturnType<{raw_type}>"),
            "",
            insertions,
        );
    } else {
        if func.params.len() != 1 {
            return;
        }
        let param = match func.params.first_pat() {
            Some(param) => param,
            None => return,
        };
        if pat_has_type_ann(param) {
            return;
        }
        if fnlike_jsdoc_present(source, func) {
            return;
        }
        push_insertion(
            insertions,
            func.node_start,
            format!("/** @type {{{raw_type}}} */ "),
        );
    }
}

/// `.js`-only: wraps a variable initializer with a JSDoc `@type` cast,
/// `/** @type {T} */ (` ... `)`.  Mirrors upstream's `addJsDocTypeToVariable`.
fn add_jsdoc_type_to_variable(
    source: &str,
    decl: &VarDeclarator,
    type_expr: &str,
    insertions: &mut Vec<Insertion>,
) {
    let Some(init) = &decl.init else {
        return;
    };
    let start = span_lo(init.span());
    let end = expr_end_before_semi(source, span_hi(init.span()));
    push_insertion(insertions, start, format!("/** @type {{{type_expr}}} */ ("));
    push_insertion(insertions, end, ")".to_string());
}

/// `.js`-only: wraps a variable initializer with a JSDoc `@satisfies` cast,
/// `/** @satisfies {T} */ (` ... `)`.  Mirrors `addJsDocSatisfiesToVariable`.
fn add_jsdoc_satisfies_to_variable(
    source: &str,
    decl: &VarDeclarator,
    type_expr: &str,
    insertions: &mut Vec<Insertion>,
) {
    let Some(init) = &decl.init else {
        return;
    };
    let start = span_lo(init.span());
    let end = expr_end_before_semi(source, span_hi(init.span()));
    push_insertion(
        insertions,
        start,
        format!("/** @satisfies {{{type_expr}}} */ ("),
    );
    push_insertion(insertions, end, ")".to_string());
}

fn expr_contains_satisfies(source: &str, span: Span) -> bool {
    let start = span_lo(span);
    let end = span_hi(span);
    source
        .get(start..end)
        .is_some_and(|slice| slice.contains("satisfies"))
}

fn find_arrow_pos(source: &str, start: usize, end: usize) -> Option<usize> {
    let slice = source.get(start..end)?;
    slice.find("=>").map(|idx| start + idx)
}

fn adjust_param_insert_pos(source: &str, pos: usize) -> usize {
    if pos == 0 {
        return pos;
    }
    if let Some(byte) = source.as_bytes().get(pos - 1) {
        if *byte == b')' {
            return pos - 1;
        }
    }
    pos
}

fn adjust_return_type_insert_pos(source: &str, pos: usize) -> usize {
    if pos == 0 {
        return pos;
    }
    if let Some(byte) = source.as_bytes().get(pos - 1) {
        if *byte == b'{' {
            return pos - 1;
        }
    }
    pos
}

fn adjust_return_type_for_async(return_type: &str, is_async: bool) -> String {
    if !is_async {
        return return_type.to_string();
    }
    let trimmed = return_type.trim();
    if trimmed.starts_with("Promise<") {
        return return_type.to_string();
    }
    if trimmed.starts_with("Awaited<") {
        return format!("Promise<{}>", return_type);
    }
    format!("Promise<Awaited<{}>>", return_type)
}

fn expr_start_with_async(source: &str, start: usize) -> usize {
    if let Some(slice) = source.get(start..start + 5) {
        if slice == "async" {
            return start;
        }
    }
    if start > 0 {
        if let Some(slice) = source.get(start - 1..start + 4) {
            if slice == "async" {
                return start - 1;
            }
        }
    }
    let mut idx = start;
    while idx > 0 {
        let b = source.as_bytes()[idx - 1];
        if !b.is_ascii_whitespace() {
            break;
        }
        idx -= 1;
    }
    if idx >= 5 {
        if let Some(slice) = source.get(idx - 5..idx) {
            if slice == "async" {
                return idx - 5;
            }
        }
    }
    start
}

fn expr_end_before_semi(source: &str, end: usize) -> usize {
    let mut idx = end;
    while idx > 0 {
        let b = source.as_bytes()[idx - 1];
        if b.is_ascii_whitespace() {
            idx -= 1;
            continue;
        }
        if b == b';' {
            idx -= 1;
        }
        break;
    }
    idx
}

fn pat_has_type_ann(pat: &Pat) -> bool {
    match pat {
        Pat::Ident(ident) => ident.type_ann.is_some(),
        Pat::Array(arr) => arr.type_ann.is_some(),
        Pat::Object(obj) => obj.type_ann.is_some(),
        Pat::Assign(assign) => pat_has_type_ann(&assign.left),
        Pat::Rest(rest) => rest.type_ann.is_some() || pat_has_type_ann(&rest.arg),
        _ => false,
    }
}

/// Returns the identifier name of a (possibly defaulted) simple parameter
/// pattern, e.g. `e` in `(e)` or `(e = {})`.  Destructured / rest patterns
/// have no single identifier, so they fall back to `arg0` at the call site.
fn pat_ident_name(pat: &Pat) -> Option<&str> {
    match pat {
        Pat::Ident(ident) => Some(ident.id.sym.as_str()),
        Pat::Assign(assign) => pat_ident_name(&assign.left),
        _ => None,
    }
}

/// Returns the text of the block comment `/** ... */` that immediately
/// precedes `node_start` (modulo trailing whitespace), or `None` if the
/// nearest preceding non-whitespace text is not the close of a block comment.
///
/// Block comments can't nest, so the last `/*` before the trailing `*/` is the
/// comment's opener — no statement-boundary heuristic is needed (and `{`/`}`
/// inside the comment or its type expressions must not be mistaken for
/// boundaries).  A line comment (`// ...`) is correctly rejected because the
/// preceding slice won't end in `*/`.
fn leading_block_comment(source: &str, node_start: usize) -> Option<&str> {
    let preceding = source.get(..node_start.min(source.len()))?;
    let trimmed = preceding.trim_end();
    let without_close = trimmed.strip_suffix("*/")?;
    let open = without_close.rfind("/*")?;
    Some(&without_close[open..])
}

/// JSDoc-already-present guard for the `.js` path.  swc does not surface
/// JSDoc as a type annotation, so without this textual check the transform
/// would double-inject on functions/variables the user already typed via
/// `/** @type ... */`, `/** @param ... */`, or `/** @satisfies ... */`
/// (mirrors upstream's shared `hasTypeDefinition` in `findExports`, which
/// for non-TS files consults `ts.getJSDocType` and checks
/// `ts.getJSDocTags(...).some(t => t.tagName.text === 'satisfies')`, plus
/// `hasTypedParameter`, which consults `ts.getJSDocParameterTags`).
///
/// Only a block comment that directly precedes the node (modulo whitespace)
/// counts, so it can't match an unrelated JSDoc earlier in the file.
fn jsdoc_type_present_before(source: &str, node_start: usize) -> bool {
    let Some(comment) = leading_block_comment(source, node_start) else {
        return false;
    };
    comment.contains("@type") || comment.contains("@param") || comment.contains("@satisfies")
}

/// True when a leading JSDoc block comment immediately preceding the node at
/// `node_start` carries a `@satisfies` tag.  swc strips comments, so the tag
/// never lands inside any AST node span — the `expr_contains_satisfies`
/// guard (which inspects the initializer's source slice for the `satisfies`
/// *operator*) cannot see it.  Mirrors upstream's `!isTsFile && getJSDocTags`
/// `@satisfies` clause that feeds the shared `hasTypeDefinition` gate.
///
/// `node_start` must be the offset whose immediately-preceding non-whitespace
/// text is the comment close — for an `export const` declaration that is the
/// `export` keyword offset (`export_start`), since the `export`/`const`
/// keywords sit between the comment and the declarator name.
fn has_leading_jsdoc_satisfies(source: &str, node_start: usize) -> bool {
    leading_block_comment(source, node_start).is_some_and(|c| c.contains("@satisfies"))
}

fn pat_end(pat: &Pat) -> Option<usize> {
    Some(span_hi(pat.span()))
}

fn span_lo(span: Span) -> usize {
    // swc BytePos is 1-indexed, convert to 0-indexed for string slicing
    (span.lo.0 as usize).saturating_sub(1)
}

fn span_hi(span: Span) -> usize {
    // swc BytePos is 1-indexed, convert to 0-indexed for string slicing
    (span.hi.0 as usize).saturating_sub(1)
}

#[derive(Debug)]
struct Insertion {
    pos: usize,
    text: String,
    order: usize,
    /// If set, delete from `pos` to `delete_until` before inserting text.
    delete_until: Option<usize>,
}

fn push_insertion(insertions: &mut Vec<Insertion>, pos: usize, text: String) {
    let order = insertions.len();
    insertions.push(Insertion {
        pos,
        text,
        order,
        delete_until: None,
    });
}

fn push_replacement(insertions: &mut Vec<Insertion>, start: usize, end: usize, text: String) {
    let order = insertions.len();
    insertions.push(Insertion {
        pos: start,
        text,
        order,
        delete_until: Some(end),
    });
}

// ============================================================================
// Promise.all empty array handling
// ============================================================================
//
// tsgo has an inference bug where `Promise.all([cond ? fetch() : []])` infers
// the empty array branch as `never[]`. This visitor finds such patterns and
// rewrites `[]` to `__svelte_empty_array(() => (other_branch))` which uses the
// other branch's type for inference.

const EMPTY_ARRAY_HELPER: &str =
    "declare function __svelte_empty_array<T>(value: () => T): Awaited<T>;\n";

/// AST visitor that finds Promise.all calls with conditional empty arrays.
struct PromiseAllVisitor<'a> {
    source: &'a str,
    replacements: Vec<(usize, usize, String)>,
}

impl<'a> PromiseAllVisitor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            replacements: Vec::new(),
        }
    }

    /// Extracts the source text for a span.
    fn span_text(&self, span: Span) -> Option<&'a str> {
        self.source.get(span_lo(span)..span_hi(span))
    }

    /// If the conditional has an empty array branch, returns (start, end, replacement).
    fn empty_array_replacement(&self, cond: &CondExpr) -> Option<(usize, usize, String)> {
        let (empty_branch, other_branch) =
            match (is_empty_array(&cond.cons), is_empty_array(&cond.alt)) {
                (true, false) => (&cond.cons, &cond.alt),
                (false, true) => (&cond.alt, &cond.cons),
                _ => return None,
            };

        let other_text = self.span_text(other_branch.span())?;
        let replacement = format!("__svelte_empty_array(() => ({}))", other_text);

        Some((
            span_lo(empty_branch.span()),
            span_hi(empty_branch.span()),
            replacement,
        ))
    }
}

impl Visit for PromiseAllVisitor<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        call.visit_children_with(self);

        if !is_promise_all_call(call) {
            return;
        }

        let Some(ExprOrSpread {
            expr: first_arg,
            spread: None,
        }) = call.args.first()
        else {
            return;
        };
        let Expr::Array(array) = first_arg.as_ref() else {
            return;
        };

        for elem in &array.elems {
            let Some(ExprOrSpread { expr, spread: None }) = elem else {
                continue;
            };
            if let Expr::Cond(cond) = expr.as_ref() {
                if let Some(replacement) = self.empty_array_replacement(cond) {
                    self.replacements.push(replacement);
                }
            }
        }
    }
}

/// Checks if a call expression is `Promise.all(...)`.
fn is_promise_all_call(call: &CallExpr) -> bool {
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Member(member) = callee.as_ref() else {
        return false;
    };
    let Expr::Ident(obj) = member.obj.as_ref() else {
        return false;
    };
    let MemberProp::Ident(prop) = &member.prop else {
        return false;
    };

    obj.sym.as_ref() == "Promise" && prop.sym.as_ref() == "all"
}

/// Check if an expression is an empty array literal []
fn is_empty_array(expr: &Expr) -> bool {
    matches!(expr, Expr::Array(ArrayLit { elems, .. }) if elems.is_empty())
}

/// Finds Promise.all calls with conditional empty arrays and generates insertions.
fn find_promise_all_empty_arrays(module: &Module, source: &str, insertions: &mut Vec<Insertion>) {
    let mut visitor = PromiseAllVisitor::new(source);
    module.visit_with(&mut visitor);

    if visitor.replacements.is_empty() {
        return;
    }

    push_insertion(insertions, 0, EMPTY_ARRAY_HELPER.to_string());

    for (start, end, text) in visitor.replacements {
        push_replacement(insertions, start, end, text);
    }
}

/// Transform any TypeScript file to fix Promise.all empty array inference issues.
/// Returns None if no transformation is needed.
pub(crate) fn transform_promise_all_empty_arrays(path: &Utf8Path, source: &str) -> Option<String> {
    // Quick check to avoid parsing files that don't need transformation
    if !source.contains("Promise.all") {
        return None;
    }

    let is_ts = matches!(path.extension(), Some("ts") | Some("tsx"));
    let module = parse_module(path, source, is_ts)?;
    let mut insertions: Vec<Insertion> = Vec::new();

    find_promise_all_empty_arrays(&module, source, &mut insertions);

    if insertions.is_empty() {
        return None;
    }

    Some(apply_insertions(source, insertions))
}

fn apply_insertions(source: &str, mut insertions: Vec<Insertion>) -> String {
    insertions.sort_by(|a, b| a.pos.cmp(&b.pos).then(a.order.cmp(&b.order)));
    let mut out = String::with_capacity(
        source.len() + insertions.iter().map(|i| i.text.len()).sum::<usize>(),
    );
    let mut last = 0;
    for ins in insertions {
        if ins.pos > source.len() {
            continue;
        }
        // Don't go backwards (can happen with overlapping replacements)
        if ins.pos < last {
            continue;
        }
        out.push_str(&source[last..ins.pos]);
        out.push_str(&ins.text);
        // If this is a replacement, skip the deleted portion
        last = ins.delete_until.unwrap_or(ins.pos);
    }
    out.push_str(&source[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn test_kit_file_kind_recognizes_hooks_with_backslash_separators() {
        // On Windows, `WalkDir` yields paths with `\` separators and
        // `Utf8Path::as_str()` preserves them.  Gated to Windows because on
        // Unix `Utf8Path::new("C:\\project\\...")` is a single-component
        // path that fools the assertions for the wrong reason.
        let root = Utf8Path::new("C:\\project");
        let cases = [
            (
                "C:\\project\\src\\hooks.server.ts",
                KitFileKind::ServerHooks,
            ),
            (
                "C:\\project\\src\\hooks.client.ts",
                KitFileKind::ClientHooks,
            ),
            ("C:\\project\\src\\hooks.ts", KitFileKind::UniversalHooks),
            ("C:\\project\\src\\params\\slug.ts", KitFileKind::Params),
        ];
        for (raw, expected) in cases {
            let path = Utf8Path::new(raw);
            let kind = kit_file_kind(path, root)
                .unwrap_or_else(|| panic!("expected kit_file_kind to recognize {raw}, got None"));
            assert!(
                std::mem::discriminant(&kind) == std::mem::discriminant(&expected),
                "expected {expected:?} for {raw}, got {kind:?}"
            );
        }
    }

    #[test]
    fn test_kit_file_kind_params_rejects_nested_src_params() {
        // A vendored library at `src/lib/vendored/pkg/src/params/match.ts`
        // is not a SvelteKit param matcher and must not get the kit
        // `ParamMatcher` type augmentation injected.
        let root = Utf8Path::new("/project");
        let path = Utf8Path::new("/project/src/lib/vendored/pkg/src/params/match.ts");
        assert!(
            kit_file_kind(path, root).is_none(),
            "nested src/params/ must not be classified as KitFileKind::Params"
        );
    }

    #[test]
    fn test_kit_file_kind_params_accepts_root_src_params() {
        // The real SvelteKit params dir lives directly under the project's
        // `src/`, so the anchored check still accepts the legitimate case.
        let root = Utf8Path::new("/project");
        let path = Utf8Path::new("/project/src/params/slug.ts");
        assert!(matches!(
            kit_file_kind(path, root),
            Some(KitFileKind::Params)
        ));
    }

    #[test]
    fn test_kit_file_kind_recognizes_hooks_with_forward_slashes() {
        let root = Utf8Path::new("/project");
        let path = Utf8Path::new("/project/src/hooks.server.ts");
        assert!(matches!(
            kit_file_kind(path, root),
            Some(KitFileKind::ServerHooks)
        ));
    }

    #[test]
    fn test_kit_file_kind_recognizes_mjs_cjs_mts_cts_hooks() {
        // ESM-flagged hooks (`.mts`/`.mjs`) and CommonJS-flagged hooks
        // (`.cts`/`.cjs`) are valid in modern Node packages and SvelteKit
        // projects with `"type": "module"`.
        let root = Utf8Path::new("/project");
        for ext in ["mts", "cts", "mjs", "cjs"] {
            let path_buf = camino::Utf8PathBuf::from(format!("/project/src/hooks.server.{ext}"));
            assert!(
                matches!(
                    kit_file_kind(&path_buf, root),
                    Some(KitFileKind::ServerHooks)
                ),
                "expected ServerHooks for .{ext}"
            );
            let path_buf = camino::Utf8PathBuf::from(format!("/project/src/hooks.client.{ext}"));
            assert!(
                matches!(
                    kit_file_kind(&path_buf, root),
                    Some(KitFileKind::ClientHooks)
                ),
                "expected ClientHooks for .{ext}"
            );
            let path_buf = camino::Utf8PathBuf::from(format!("/project/src/hooks.{ext}"));
            assert!(
                matches!(
                    kit_file_kind(&path_buf, root),
                    Some(KitFileKind::UniversalHooks)
                ),
                "expected UniversalHooks for .{ext}"
            );
        }
    }

    #[test]
    fn test_kit_file_kind_recognizes_mts_params() {
        let root = Utf8Path::new("/project");
        let path = Utf8Path::new("/project/src/params/slug.mts");
        assert!(matches!(
            kit_file_kind(path, root),
            Some(KitFileKind::Params)
        ));
    }

    #[test]
    fn test_is_kit_script_ext_covers_modern_extensions() {
        for ext in ["ts", "js", "mts", "cts", "mjs", "cjs"] {
            assert!(is_kit_script_ext(ext), "expected {ext} to be recognized");
        }
        for ext in ["svelte", "json", "css", "txt", "tsx", "jsx"] {
            assert!(!is_kit_script_ext(ext), "expected {ext} to be rejected");
        }
    }

    #[test]
    fn test_promise_all_empty_array_transform() {
        let source = r#"const [a, b] = await Promise.all([
    cond ? foo() : [],
    other()
]);"#;
        let path = Utf8Path::new("test.ts");
        let result = transform_promise_all_empty_arrays(path, source).expect("should transform");

        assert!(result.contains("__svelte_empty_array"));
        assert!(result.contains("__svelte_empty_array(() => (foo()))"));
        assert!(!result.contains(": []"));
    }

    #[test]
    fn test_promise_all_no_transform_needed() {
        let source = "const x = await Promise.all([foo(), bar()]);";
        let path = Utf8Path::new("test.ts");
        let result = transform_promise_all_empty_arrays(path, source);

        assert!(result.is_none());
    }

    #[test]
    fn test_span_indexing() {
        let source = "const x = [];";
        let path = Utf8Path::new("test.ts");
        let module = parse_module(path, source, true).unwrap();

        struct SpanChecker<'a>(&'a str, bool);
        impl Visit for SpanChecker<'_> {
            fn visit_array_lit(&mut self, arr: &ArrayLit) {
                let slice = self.0.get(span_lo(arr.span())..span_hi(arr.span()));
                assert_eq!(slice, Some("[]"));
                self.1 = true;
            }
        }

        let mut checker = SpanChecker(source, false);
        module.visit_with(&mut checker);
        assert!(checker.1, "should have visited array literal");
    }

    // -----------------------------------------------------------------
    // Params transform: regression coverage for inferred-type-predicate
    // preservation.  TypeScript 5.5+ infers `(p: string) => p is "a" | "b"`
    // from `(p: string) => p === "a" || p === "b"`.  Earlier versions of
    // this transform forced `: boolean` onto the matcher's return type,
    // which killed predicate inference and broke SvelteKit's
    // `MatcherParam<typeof match>` route-param narrowing.  These tests pin
    // the new behavior so it doesn't regress.
    // -----------------------------------------------------------------

    fn root() -> &'static Utf8Path {
        Utf8Path::new("/project")
    }

    fn params_path() -> &'static Utf8Path {
        Utf8Path::new("/project/src/params/slug.ts")
    }

    fn transform_params(source: &str) -> String {
        let path = params_path();
        let kind = kit_file_kind(path, root()).expect("kit_file_kind for params/");
        transform_kit_source(kind, path, source)
            .expect("transform_kit_source should return Some for params/")
    }

    #[test]
    fn test_params_matcher_arrow_no_forced_boolean_return() {
        let source = "export const match = (param: string) => param === 'a' || param === 'b';";
        let out = transform_params(source);
        assert!(
            !out.contains(": boolean"),
            "transform must not inject `: boolean` return type \
             (it would defeat TS 5.5+ inferred type predicates).\n{out}"
        );
        // Param annotation is already present, so the user's source survives.
        assert!(out.contains("(param: string)"), "unexpected output:\n{out}");
    }

    #[test]
    fn test_params_matcher_arrow_param_annotated_when_missing() {
        // Bare arrow with no param annotation — transform should add `: string`
        // (which doesn't interfere with predicate inference) but still not the
        // boolean return.
        let source = "export const match = (param) => param === 'a';";
        let out = transform_params(source);
        assert!(
            out.contains("(param: string)"),
            "should inject `: string`:\n{out}"
        );
        assert!(
            !out.contains(": boolean"),
            "must not inject `: boolean`:\n{out}"
        );
    }

    #[test]
    fn test_params_matcher_function_decl_no_forced_boolean_return() {
        let source = "export function match(param: string) { return param === 'a'; }";
        let out = transform_params(source);
        assert!(
            !out.contains(": boolean"),
            "function-declaration form must also skip `: boolean`:\n{out}"
        );
    }

    #[test]
    fn test_params_matcher_appends_satisfies_check() {
        let source = "export const match = (param: string) => param === 'a';";
        let out = transform_params(source);
        // The constraint check must reference the user's `match` symbol and
        // SvelteKit's `ParamMatcher` — that's how a wrong return type
        // (e.g. `(p) => p` returning string) still surfaces as a TS1360.
        assert!(
            out.contains("match satisfies import('@sveltejs/kit').ParamMatcher"),
            "should append ParamMatcher constraint:\n{out}"
        );
        // Use `void (...)` to avoid TS6133 / TS2304 on noUnusedExpressions.
        assert!(
            out.contains("void (match satisfies"),
            "should wrap satisfies in `void` to suppress unused-expression diagnostics:\n{out}"
        );
    }

    #[test]
    fn test_params_matcher_satisfies_appended_after_function_body() {
        // The satisfies check must come AFTER the function definition so
        // `match` is in scope.  Earlier position would be a TS2448
        // (block-scoped variable used before its declaration).
        let source = "export const match = (param: string) => param === 'a';";
        let out = transform_params(source);
        let match_pos = out
            .find("export const match")
            .expect("export const match must be present");
        let satisfies_pos = out
            .find("match satisfies import('@sveltejs/kit').ParamMatcher")
            .expect("satisfies check must be present");
        assert!(
            satisfies_pos > match_pos,
            "satisfies check at byte {satisfies_pos} must come after match decl at byte {match_pos}:\n{out}"
        );
    }

    #[test]
    fn test_params_no_match_export_skips_transform() {
        // Params file without a `match` export — nothing to constrain.
        let source = "export const other = 1;";
        let path = params_path();
        let kind = kit_file_kind(path, root()).unwrap();
        let result = transform_kit_source(kind, path, source);
        // No insertions → returns None.
        assert!(
            result.is_none(),
            "should be a no-op when there's no `match` export"
        );
    }

    // -----------------------------------------------------------------
    // Route transform: respect explicit type annotations on HTTP method
    // exports so the user's choice of `RequestHandler` (broad vs route-
    // specific) is preserved.  Adding an inner `params: RequestEvent`
    // annotation when the outer already says `RequestHandler` from
    // `@sveltejs/kit` silently overrides the loose typing and masks
    // legitimate `params.X is string | undefined` errors.
    // -----------------------------------------------------------------

    fn server_path() -> &'static Utf8Path {
        Utf8Path::new("/project/src/routes/foo/+server.ts")
    }

    fn transform_server(source: &str) -> Option<String> {
        let path = server_path();
        let kind = kit_file_kind(path, root()).expect("kit_file_kind for +server.ts");
        transform_kit_source(kind, path, source)
    }

    #[test]
    fn test_http_method_skips_param_annotation_when_outer_typed() {
        // `RequestHandler` is the broad `@sveltejs/kit` shape; the user
        // chose it deliberately.  We must NOT inject an inner annotation
        // that overrides it — when nothing else needs transforming the
        // transformer returns None, which the orchestrator treats as
        // "use the original source unchanged".
        let source = "\
import type { RequestHandler } from '@sveltejs/kit';
export const GET: RequestHandler = async ({ params }) => new Response(params.foo);
";
        let out = transform_server(source).unwrap_or_else(|| source.to_string());
        assert!(
            !out.contains("import('./$types.js').RequestEvent"),
            "must not inject RequestEvent annotation when outer is typed:\n{out}"
        );
    }

    #[test]
    fn test_http_method_adds_param_annotation_when_outer_untyped() {
        // Without an outer annotation, the user gets the loose
        // `({locals, params}: any)` shape — exactly the case where the
        // RequestEvent annotation is legitimately useful.
        let source = "\
export const GET = async ({ params }) => new Response(params.foo);
";
        let out = transform_server(source).expect("should transform untyped GET");
        assert!(
            out.contains("import('./$types.js').RequestEvent"),
            "should inject RequestEvent annotation when outer is untyped:\n{out}"
        );
    }

    #[test]
    fn test_http_method_fn_decl_still_annotates() {
        // The guard is for `export const NAME = fn`.  Function declarations
        // can't carry an outer type annotation, so the function-form code
        // path stays unchanged.
        let source = "\
export async function GET({ params }) { return new Response(params.foo); }
";
        let out = transform_server(source).expect("should transform function decl");
        assert!(
            out.contains("import('./$types.js').RequestEvent"),
            "function declaration should still get RequestEvent annotation:\n{out}"
        );
    }

    // -----------------------------------------------------------------
    // JSDoc-flavoured SvelteKit transforms for `.js` route/hooks/params
    // files.  Mirrors svelte2tsx/test/helpers/index.ts (the eight new
    // "with jsdoc" cases) from upstream commit b914d010.  For `.js` files
    // the transform must emit JSDoc comments instead of TS type-annotation
    // syntax, otherwise tsgo reports TS8010 on the generated `.js` file.
    // -----------------------------------------------------------------

    /// Maps an upstream `upsert(file, ...)` test path to a project-rooted
    /// path the `kit_file_kind` classifier accepts.
    fn kit_path_for(file: &str) -> camino::Utf8PathBuf {
        let base = "/project";
        match file {
            "hooks.server.js" | "hooks.server.ts" => {
                camino::Utf8PathBuf::from(format!("{base}/src/{file}"))
            }
            "hooks.client.js" | "hooks.client.ts" | "hooks.js" | "hooks.ts" => {
                camino::Utf8PathBuf::from(format!("{base}/src/{file}"))
            }
            _ => camino::Utf8PathBuf::from(format!("{base}/src/routes/{file}")),
        }
    }

    fn upsert(file: &str, source: &str, expected: &str) {
        let path = kit_path_for(file);
        let kind = kit_file_kind(&path, root())
            .unwrap_or_else(|| panic!("kit_file_kind must classify {file}"));
        let out = transform_kit_source(kind, &path, source).unwrap_or_else(|| source.to_string());
        assert_eq!(out, expected, "upsert mismatch for {file}");
    }

    #[test]
    fn test_jsdoc_page_load_function() {
        upsert(
            "+page.js",
            "export function load(e) { return e; }",
            "/** @param {import('./$types.js').PageLoadEvent} e */ export function load(e) { return e; }",
        );
    }

    #[test]
    fn test_jsdoc_page_load_function_with_jsdoc_type_left_as_is() {
        // Upstream "leaves +page.js function with jsdoc as is #1": an existing
        // `@type` JSDoc means hasTypeDefinition → no re-injection.
        let src =
            "/** @type {import('./$types.js').PageLoad} */ export function load(e) { return e; }";
        upsert("+page.js", src, src);
    }

    #[test]
    fn test_jsdoc_page_load_function_with_jsdoc_param_left_as_is() {
        // Upstream "leaves +page.js function with jsdoc as is #2": an existing
        // `@param` JSDoc on the first parameter means hasTypedParameter → skip.
        let src = "/** @param {import('./$types.js').PageLoadEvent} e */ export function load(e) { return e; }";
        upsert("+page.js", src, src);
    }

    #[test]
    fn test_jsdoc_handle_hook_const() {
        upsert(
            "hooks.server.js",
            "export const handle = async ({ event, resolve }) => {};",
            "export const handle = /** @type {import('@sveltejs/kit').Handle} */ async ({ event, resolve }) => {};",
        );
    }

    #[test]
    fn test_jsdoc_get_async_function() {
        upsert(
            "+server.js",
            "export async function GET(e) {}",
            "/** @type {(arg0: import('./$types.js').RequestEvent) => Response | Promise<Response>} */ export async function GET(e) {}",
        );
    }

    #[test]
    fn test_jsdoc_load_const_with_paranthesis() {
        upsert(
            "+page.js",
            "export const load = (async (e) => {});",
            "export const load = (/** @param {import('./$types.js').PageLoadEvent} e */ async (e) => {});",
        );
    }

    #[test]
    fn test_jsdoc_actions() {
        upsert(
            "+page.server.js",
            "export const actions = { default: async (e) => {} };",
            "export const actions = /** @satisfies {import('./$types.js').Actions} */ ({ default: async (e) => {} });",
        );
    }

    #[test]
    fn test_jsdoc_page_at_ssr() {
        upsert(
            "+page@.js",
            "export const ssr = true;",
            "export const ssr = /** @type {boolean} */ (true);",
        );
    }

    #[test]
    fn test_jsdoc_layout_at_foo_ssr() {
        upsert(
            "+layout@foo.js",
            "export const ssr = true;",
            "export const ssr = /** @type {boolean} */ (true);",
        );
    }

    // -----------------------------------------------------------------
    // Params `match` on `.js`: the parity-critical divergence from
    // upstream.  Upstream emits `@type {(arg0: string) => boolean}`, which
    // forces a boolean return and defeats TS 5.5+ inferred type predicates.
    // We instead emit `@param {string}` (param-only) plus the trailing
    // `ParamMatcher` satisfies check, preserving the repo's predicate
    // invariant while staying TS8010-free.
    // -----------------------------------------------------------------

    fn js_params_path() -> camino::Utf8PathBuf {
        camino::Utf8PathBuf::from("/project/src/params/slug.js")
    }

    fn transform_js_params(source: &str) -> String {
        let path = js_params_path();
        let kind = kit_file_kind(&path, root()).expect("kit_file_kind for params/*.js");
        transform_kit_source(kind, &path, source)
            .expect("transform_kit_source should return Some for params/")
    }

    #[test]
    fn test_jsdoc_params_match_emits_param_not_boolean_type() {
        let source = "export const match = (param) => param === 'a';";
        let out = transform_js_params(source);
        // JSDoc `@param {string}` on the arrow, NOT a TS `: string` annotation
        // (which would be TS8010 in a checkJs `.js` file).
        assert!(
            out.contains("/** @param {string} param */"),
            "should inject a JSDoc @param, got:\n{out}"
        );
        // Must NOT force a boolean return type (upstream's behaviour we reject).
        assert!(
            !out.contains("=> boolean"),
            "must not force `(arg0: string) => boolean`:\n{out}"
        );
        // No TS type-annotation syntax that would trip TS8010 in JS.
        assert!(
            !out.contains("(param: string)"),
            "must not emit a TS `: string` parameter annotation in .js:\n{out}"
        );
        // The predicate-preserving ParamMatcher check is still appended, but
        // as a JSDoc `@satisfies` cast — NOT the TS-only `satisfies` operator
        // (which would be TS8010/TS8037 in a checkJs `.js` file).
        assert!(
            out.contains("void (/** @satisfies {import('@sveltejs/kit').ParamMatcher} */ (match))"),
            "should append the ParamMatcher constraint as a JSDoc @satisfies cast:\n{out}"
        );
        // Lock in that no bare TS `satisfies` operator leaks into the .js file.
        assert!(
            !out.contains("match satisfies"),
            "must not emit the TS-only `satisfies` operator in .js (TS8010/TS8037):\n{out}"
        );
    }

    #[test]
    fn test_ts_params_matcher_uses_satisfies_operator_not_jsdoc() {
        // `.ts` keeps the bare `satisfies` operator form (no JSDoc cast).
        let source = "export const match = (param: string) => param === 'a';";
        let out = transform_params(source);
        assert!(
            out.contains("void (match satisfies import('@sveltejs/kit').ParamMatcher)"),
            "TS must use the bare `satisfies` operator form:\n{out}"
        );
        assert!(
            !out.contains("/** @satisfies"),
            "TS must NOT emit a JSDoc @satisfies cast:\n{out}"
        );
    }

    #[test]
    fn test_js_params_matcher_uses_jsdoc_satisfies_cast() {
        // `.js` injects the JSDoc @param AND appends the JSDoc @satisfies cast,
        // with no TS-only operator anywhere in the output.
        let source = "export const match = (param) => param === 'a';";
        let out = transform_js_params(source);
        assert!(
            out.contains("/** @param {string} param */"),
            "should inject the JSDoc @param annotation:\n{out}"
        );
        assert!(
            out.contains("void (/** @satisfies {import('@sveltejs/kit').ParamMatcher} */ (match))"),
            "should append the JSDoc @satisfies cast:\n{out}"
        );
        // Exactly one `@satisfies` (the trailing cast); no stray duplicates.
        assert_eq!(
            out.matches("@satisfies").count(),
            1,
            "should append exactly one @satisfies cast:\n{out}"
        );
        // No bare TS `satisfies` operator (would be TS8010/TS8037 in checkJs).
        assert!(
            !out.contains(" satisfies "),
            "must not emit the TS-only `satisfies` operator in .js:\n{out}"
        );
    }

    #[test]
    fn test_jsdoc_no_double_inject_with_existing_param_jsdoc() {
        // A `.js` param matcher already carrying a `@param` JSDoc must be
        // left untouched by the param-type injection (only the trailing
        // satisfies check is appended).
        let source = "/** @param {string} param */ export const match = (param) => param === 'a';";
        let out = transform_js_params(source);
        // Exactly one `@param` JSDoc (no duplicate injected).
        assert_eq!(
            out.matches("@param").count(),
            1,
            "must not duplicate the existing @param JSDoc:\n{out}"
        );
    }

    // -----------------------------------------------------------------
    // Existing JSDoc `@satisfies` de-dup (upstream commit d69eb726, #2946).
    // swc strips comments, so a leading `/** @satisfies {T} */` never lands
    // in any AST node span — the `expr_contains_satisfies` operator guard
    // can't see it.  Without the textual `has_leading_jsdoc_satisfies` /
    // extended `jsdoc_type_present_before` guard, the transform would inject
    // a *second*, conflicting `@satisfies` wrap.  Mirrors upstream's shared
    // `!isTsFile && getJSDocTags(...).some(@satisfies)` clause in
    // `hasTypeDefinition`.
    // -----------------------------------------------------------------

    fn page_server_js_path() -> camino::Utf8PathBuf {
        camino::Utf8PathBuf::from("/project/src/routes/+page.server.js")
    }

    fn page_js_path() -> camino::Utf8PathBuf {
        camino::Utf8PathBuf::from("/project/src/routes/+page.js")
    }

    /// Runs the kit transform for a route file, returning the resulting source
    /// (or the original unchanged when the transform is a no-op / returns None).
    fn transform_route(path: &Utf8Path, source: &str) -> String {
        let kind = kit_file_kind(path, root())
            .unwrap_or_else(|| panic!("kit_file_kind must classify {path}"));
        transform_kit_source(kind, path, source).unwrap_or_else(|| source.to_string())
    }

    #[test]
    fn test_actions_with_leading_jsdoc_satisfies_not_reinjected() {
        // Upstream fixture: "leaves actions with existing jsdoc @satisfies as is".
        let source = "/** @satisfies {import('./$types').Actions} */\n\
             export const actions = { default: async (e) => {} };";
        let out = transform_route(&page_server_js_path(), source);
        assert_eq!(
            out, source,
            "actions with leading @satisfies must be left as-is:\n{out}"
        );
        // No injected trailing/wrapping satisfies pointing at the generated
        // `$types.js` module.
        assert!(
            !out.contains("satisfies import('./$types.js').Actions"),
            "must not inject a second @satisfies for actions:\n{out}"
        );
        // Exactly one `@satisfies` survives — the user's.
        assert_eq!(
            out.matches("@satisfies").count(),
            1,
            "must not duplicate the existing @satisfies JSDoc:\n{out}"
        );
    }

    #[test]
    fn test_load_const_with_leading_jsdoc_satisfies_not_reinjected() {
        // Upstream fixture: "leaves load const with existing jsdoc @satisfies as is".
        let source = "/** @satisfies {import('./$types').PageLoad} */\n\
             export const load = (async (e) => {});";
        let out = transform_route(&page_js_path(), source);
        assert_eq!(
            out, source,
            "load const with leading @satisfies must be left as-is:\n{out}"
        );
        assert!(
            !out.contains("satisfies import('./$types.js').PageLoad"),
            "must not inject a second @satisfies for load:\n{out}"
        );
        // The function-like `@param` injection must also be suppressed.
        assert!(
            !out.contains("@param"),
            "must not inject a @param when @satisfies already types the load:\n{out}"
        );
        assert_eq!(out.matches("@satisfies").count(), 1, "{out}");
    }

    #[test]
    fn test_load_const_without_jsdoc_still_injected() {
        // Regression guard: the new gate must not be over-broad — a load const
        // with no leading JSDoc still gets its function-like `@param` injected.
        let source = "export const load = (async (e) => {});";
        let out = transform_route(&page_js_path(), source);
        assert!(
            out.contains("@param {import('./$types.js').PageLoadEvent} e"),
            "load const without JSDoc should still be annotated:\n{out}"
        );
    }

    #[test]
    fn test_actions_jsdoc_satisfies_only_applies_to_js() {
        // Faithful to upstream `!isTsFile`: on `.ts` the JSDoc `@satisfies` is
        // ignored (TS doesn't read it), and there's no expression-form
        // `satisfies` operator either, so the trailing `satisfies` IS injected.
        let ts_source = "/** @satisfies {import('./$types').Actions} */\n\
             export const actions = { default: async (e) => {} };";
        let ts_path = camino::Utf8PathBuf::from("/project/src/routes/+page.server.ts");
        let out = transform_route(&ts_path, ts_source);
        assert!(
            out.contains("satisfies import('./$types.js').Actions"),
            ".ts must still inject the trailing satisfies (JSDoc tag is JS-only):\n{out}"
        );

        // And the TS expression-form `(...) satisfies T` is still suppressed by
        // the existing `expr_contains_satisfies` guard, independent of JSDoc.
        let ts_expr_source =
            "export const actions = ({ default: async (e) => {} }) satisfies import('./$types').Actions;";
        let out_expr = transform_route(&ts_path, ts_expr_source);
        assert_eq!(out_expr.matches("satisfies").count(), 1, "{out_expr}");
    }

    #[test]
    fn test_has_leading_jsdoc_satisfies_unit() {
        // The guard is called with the `export` keyword offset (the comment's
        // immediately-following non-whitespace token), so scan from there.

        // (a) leading `@satisfies` block comment → true.
        let a = "/** @satisfies {X} */\nexport const load = (e) => {};";
        let export_off = a.find("export").unwrap();
        assert!(
            has_leading_jsdoc_satisfies(a, export_off),
            "should detect a leading @satisfies JSDoc"
        );

        // (b) leading `@type` block comment → false (not a @satisfies tag).
        let b = "/** @type {X} */\nexport const load = (e) => {};";
        assert!(
            !has_leading_jsdoc_satisfies(b, b.find("export").unwrap()),
            "@type JSDoc must not count as @satisfies"
        );

        // (c) no comment → false.
        let c = "export const load = (e) => {};";
        assert!(
            !has_leading_jsdoc_satisfies(c, c.find("export").unwrap()),
            "no comment → false"
        );

        // (d) line `//` comment containing the word "satisfies" → false (only
        // block JSDoc counts; the preceding text doesn't end in `*/`).
        let d = "// this satisfies nothing\nexport const load = (e) => {};";
        assert!(
            !has_leading_jsdoc_satisfies(d, d.find("export").unwrap()),
            "line comment must not match"
        );

        // (e) multi-line JSDoc with `@satisfies` on its own line → true.
        let e = "/**\n * @satisfies {X}\n */\nexport const load = (e) => {};";
        assert!(
            has_leading_jsdoc_satisfies(e, e.find("export").unwrap()),
            "multi-line @satisfies JSDoc should be detected"
        );
    }

    #[test]
    fn test_ts_route_unchanged_regression() {
        // Regression guard: the `.ts` path must be byte-for-byte the existing
        // behaviour (TS annotation syntax, no JSDoc).
        let path = camino::Utf8PathBuf::from("/project/src/routes/+page@.ts");
        let kind = kit_file_kind(&path, root()).unwrap();
        let out = transform_kit_source(kind, &path, "export const ssr = true;")
            .expect("should transform");
        // svelte-check-rs's existing TS path injects `: boolean` (no leading
        // space) at the end of the variable name — distinct from upstream's
        // ` : boolean`, but deliberately left unchanged by this fix.
        assert_eq!(out, "export const ssr: boolean = true;");
        assert!(!out.contains("/**"), "TS path must not emit JSDoc:\n{out}");
    }
}
