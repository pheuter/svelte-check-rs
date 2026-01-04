use camino::Utf8Path;
use std::sync::Arc;
use swc_common::{FileName, SourceMap, Span, Spanned};
use swc_ecma_ast::{
    BlockStmtOrExpr, Decl, ExportDecl, FnDecl, Function, Module, ModuleDecl, ModuleItem, Pat,
    VarDecl, VarDeclarator,
};
use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax};

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

pub(crate) fn kit_file_kind(path: &Utf8Path, project_root: &Utf8Path) -> Option<KitFileKind> {
    let ext = path.extension()?;
    if ext != "ts" && ext != "js" {
        return None;
    }

    let file_name = path.file_name()?;
    if let Some(route_kind) = kit_route_kind(file_name) {
        return Some(KitFileKind::Route(route_kind));
    }

    let rel = path.strip_prefix(project_root).ok().unwrap_or(path);
    let rel_str = rel.as_str().trim_start_matches('/');

    if rel_str.ends_with("src/hooks.server.ts") || rel_str.ends_with("src/hooks.server.js") {
        return Some(KitFileKind::ServerHooks);
    }
    if rel_str.ends_with("src/hooks.client.ts") || rel_str.ends_with("src/hooks.client.js") {
        return Some(KitFileKind::ClientHooks);
    }
    if rel_str.ends_with("src/hooks.ts") || rel_str.ends_with("src/hooks.js") {
        return Some(KitFileKind::UniversalHooks);
    }

    if rel_str.contains("src/params/")
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
    let is_ts = path.extension() == Some("ts");
    let module = parse_module(path, source, is_ts)?;
    let mut insertions: Vec<Insertion> = Vec::new();

    match kind {
        KitFileKind::Route(route_kind) => {
            apply_route_transforms(&module, source, route_kind, &mut insertions);
        }
        KitFileKind::ServerHooks => {
            apply_hooks_transforms(
                &module,
                source,
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
                &["reroute"],
                "import('@sveltejs/kit').Reroute",
                "",
                "",
                &mut insertions,
            );
        }
        KitFileKind::Params => {
            apply_params_transforms(&module, source, &mut insertions);
        }
    }

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
        if let Some(exports) = export_decl(item) {
            for export in exports {
                match export {
                    ExportDeclKind::Fn(name, func) => {
                        if name == "load" && !kind.is_endpoint {
                            add_param_type_if_missing(
                                func,
                                source,
                                &format!("import('./$types.js').{load_event}"),
                                insertions,
                            );
                        } else if name == "entries" && !kind.is_layout {
                            add_return_type_if_missing(
                                func,
                                source,
                                &format!("ReturnType<import('./$types.js').{entry_type}>"),
                                insertions,
                            );
                        } else if http_methods.contains(&name.as_str()) {
                            add_param_type_if_missing(
                                func,
                                source,
                                &format!("import('./$types.js').{request_event}"),
                                insertions,
                            );
                        }
                    }
                    ExportDeclKind::Var(name, decl) => {
                        if name == "load" && !kind.is_endpoint {
                            if !pat_has_type_ann(&decl.name) {
                                if let Some(init) = &decl.init {
                                    if expr_contains_satisfies(source, init.span()) {
                                        continue;
                                    }
                                    let start = expr_start_with_async(source, span_lo(init.span()));
                                    push_insertion(insertions, start, "(".to_string());
                                    let end = expr_end_before_semi(source, span_hi(init.span()));
                                    push_insertion(
                                        insertions,
                                        end,
                                        format!(") satisfies import('./$types.js').{load_type}"),
                                    );
                                }
                            }
                        } else if name == "actions" {
                            if !pat_has_type_ann(&decl.name) {
                                if let Some(init) = &decl.init {
                                    if expr_contains_satisfies(source, init.span()) {
                                        continue;
                                    }
                                    push_insertion(
                                        insertions,
                                        expr_end_before_semi(source, span_hi(init.span())),
                                        format!(" satisfies import('./$types.js').{actions_type}"),
                                    );
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
                                if let Some(end) = pat_end(&decl.name) {
                                    push_insertion(insertions, end, format!(": {ty}"));
                                }
                            }
                        } else if name == "entries" && !kind.is_layout {
                            if let Some(func_like) = function_like_from_expr(&decl.init) {
                                add_return_type_if_missing_fnlike(
                                    func_like,
                                    source,
                                    &format!("ReturnType<import('./$types.js').{entry_type}>"),
                                    insertions,
                                );
                            }
                        } else if http_methods.contains(&name.as_str()) {
                            if let Some(func_like) = function_like_from_expr(&decl.init) {
                                add_param_type_if_missing_fnlike(
                                    func_like,
                                    source,
                                    &format!("import('./$types.js').{request_event}"),
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

fn apply_hooks_transforms(
    module: &Module,
    source: &str,
    names: &[&str],
    handle_error_type: &str,
    handle_type: &str,
    handle_fetch_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    for item in &module.body {
        if let Some(exports) = export_decl(item) {
            for export in exports {
                match export {
                    ExportDeclKind::Fn(name, func) => {
                        if name == "handleError" && names.contains(&"handleError") {
                            add_param_and_return_if_missing(
                                func,
                                source,
                                &format!("Parameters<{handle_error_type}>[0]"),
                                &format!("ReturnType<{handle_error_type}>"),
                                insertions,
                            );
                        } else if name == "handle" && names.contains(&"handle") {
                            add_param_and_return_if_missing(
                                func,
                                source,
                                &format!("Parameters<{handle_type}>[0]"),
                                &format!("ReturnType<{handle_type}>"),
                                insertions,
                            );
                        } else if name == "handleFetch" && names.contains(&"handleFetch") {
                            add_param_and_return_if_missing(
                                func,
                                source,
                                &format!("Parameters<{handle_fetch_type}>[0]"),
                                &format!("ReturnType<{handle_fetch_type}>"),
                                insertions,
                            );
                        } else if name == "reroute" && names.contains(&"reroute") {
                            add_param_and_return_if_missing(
                                func,
                                source,
                                &format!("Parameters<{handle_error_type}>[0]"),
                                &format!("ReturnType<{handle_error_type}>"),
                                insertions,
                            );
                        }
                    }
                    ExportDeclKind::Var(name, decl) => {
                        if let Some(func_like) = function_like_from_expr(&decl.init) {
                            if name == "handleError" && names.contains(&"handleError") {
                                add_param_and_return_if_missing_fnlike(
                                    func_like,
                                    source,
                                    &format!("Parameters<{handle_error_type}>[0]"),
                                    &format!("ReturnType<{handle_error_type}>"),
                                    insertions,
                                );
                            } else if name == "handle" && names.contains(&"handle") {
                                add_param_and_return_if_missing_fnlike(
                                    func_like,
                                    source,
                                    &format!("Parameters<{handle_type}>[0]"),
                                    &format!("ReturnType<{handle_type}>"),
                                    insertions,
                                );
                            } else if name == "handleFetch" && names.contains(&"handleFetch") {
                                add_param_and_return_if_missing_fnlike(
                                    func_like,
                                    source,
                                    &format!("Parameters<{handle_fetch_type}>[0]"),
                                    &format!("ReturnType<{handle_fetch_type}>"),
                                    insertions,
                                );
                            } else if name == "reroute" && names.contains(&"reroute") {
                                add_param_and_return_if_missing_fnlike(
                                    func_like,
                                    source,
                                    &format!("Parameters<{handle_error_type}>[0]"),
                                    &format!("ReturnType<{handle_error_type}>"),
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

fn apply_params_transforms(module: &Module, source: &str, insertions: &mut Vec<Insertion>) {
    for item in &module.body {
        if let Some(exports) = export_decl(item) {
            for export in exports {
                match export {
                    ExportDeclKind::Fn(name, func) => {
                        if name == "match" {
                            add_param_and_return_if_missing(
                                func, source, "string", "boolean", insertions,
                            );
                        }
                    }
                    ExportDeclKind::Var(name, decl) => {
                        if name == "match" {
                            if let Some(func_like) = function_like_from_expr(&decl.init) {
                                add_param_and_return_if_missing_fnlike(
                                    func_like, source, "string", "boolean", insertions,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

enum ExportDeclKind<'a> {
    Fn(String, &'a Function),
    Var(String, &'a VarDeclarator),
}

fn export_decl(item: &ModuleItem) -> Option<Vec<ExportDeclKind<'_>>> {
    match item {
        ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl { decl, .. })) => {
            export_from_decl(decl)
        }
        _ => None,
    }
}

fn export_from_decl(decl: &Decl) -> Option<Vec<ExportDeclKind<'_>>> {
    match decl {
        Decl::Fn(FnDecl {
            ident, function, ..
        }) => Some(vec![ExportDeclKind::Fn(ident.sym.to_string(), function)]),
        Decl::Var(var) => export_from_var_decl(var),
        _ => None,
    }
}

fn export_from_var_decl(decl: &VarDecl) -> Option<Vec<ExportDeclKind<'_>>> {
    let exports: Vec<_> = decl
        .decls
        .iter()
        .filter_map(|d| {
            if let Pat::Ident(ident) = &d.name {
                Some(ExportDeclKind::Var(ident.id.sym.to_string(), d))
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
    if let Some(pos) = pat_end(param) {
        let insert_pos = adjust_param_insert_pos(source, pos);
        push_insertion(insertions, insert_pos, format!(": {type_expr}"));
    }
}

fn add_return_type_if_missing(
    func: &Function,
    source: &str,
    return_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    if func.return_type.is_some() {
        return;
    }
    if let Some(body) = &func.body {
        let adjusted_type = adjust_return_type_for_async(return_type, func.is_async);
        let insert_pos = adjust_return_type_insert_pos(source, span_lo(body.span()));
        push_insertion(insertions, insert_pos, format!(": {adjusted_type} "));
    }
}

fn add_param_and_return_if_missing(
    func: &Function,
    source: &str,
    param_type: &str,
    return_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    add_param_type_if_missing(func, source, param_type, insertions);
    add_return_type_if_missing(func, source, return_type, insertions);
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
    is_arrow: bool,
    is_async: bool,
}

fn function_like_from_expr(expr: &Option<Box<swc_ecma_ast::Expr>>) -> Option<FunctionLike<'_>> {
    match expr.as_deref()? {
        swc_ecma_ast::Expr::Arrow(arrow) => {
            let body_start = match arrow.body.as_ref() {
                BlockStmtOrExpr::BlockStmt(block) => span_lo(block.span()),
                BlockStmtOrExpr::Expr(expr) => span_lo(expr.span()),
            };
            Some(FunctionLike {
                params: FunctionLikeParams::Arrow(&arrow.params),
                return_type: arrow.return_type.as_deref(),
                body_start,
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
    if let Some(pos) = pat_end(param) {
        let insert_pos = adjust_param_insert_pos(source, pos);
        push_insertion(insertions, insert_pos, format!(": {param_type}"));
    }
}

fn add_return_type_if_missing_fnlike(
    func: FunctionLike<'_>,
    source: &str,
    return_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    if func.return_type.is_some() {
        return;
    }
    let adjusted_type = adjust_return_type_for_async(return_type, func.is_async);
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
}

fn add_param_and_return_if_missing_fnlike(
    func: FunctionLike<'_>,
    source: &str,
    param_type: &str,
    return_type: &str,
    insertions: &mut Vec<Insertion>,
) {
    add_param_type_if_missing_fnlike(func, source, param_type, insertions);
    add_return_type_if_missing_fnlike(func, source, return_type, insertions);
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

fn pat_end(pat: &Pat) -> Option<usize> {
    Some(span_hi(pat.span()))
}

fn span_lo(span: Span) -> usize {
    span.lo.0 as usize
}

fn span_hi(span: Span) -> usize {
    span.hi.0 as usize
}

#[derive(Debug)]
struct Insertion {
    pos: usize,
    text: String,
    order: usize,
}

fn push_insertion(insertions: &mut Vec<Insertion>, pos: usize, text: String) {
    let order = insertions.len();
    insertions.push(Insertion { pos, text, order });
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
        out.push_str(&source[last..ins.pos]);
        out.push_str(&ins.text);
        last = ins.pos;
    }
    out.push_str(&source[last..]);
    out
}
