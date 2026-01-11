use crate::{Diagnostic, DiagnosticCode};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use source_map::Span as SourceSpan;
use std::sync::Arc;
use svelte_parser::{Script, ScriptLang, SvelteDocument};
use swc_common::{BytePos, FileName, SourceMap, Span as SwcSpan};
use swc_ecma_ast::*;
use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax};
use swc_ecma_visit::{Visit, VisitWith};

pub fn check(doc: &SvelteDocument) -> Vec<Diagnostic> {
    let Some(script) = doc.instance_script.as_ref() else {
        return Vec::new();
    };

    check_script(script)
}

fn check_script(script: &Script) -> Vec<Diagnostic> {
    let is_ts = script.lang == ScriptLang::TypeScript;
    let Some(program) = parse_program(&script.content, is_ts) else {
        return Vec::new();
    };

    let base_offset: u32 = script.content_span.start.into();
    let mut analyzer = StateReferenceAnalyzer::new(program.file_start, base_offset);
    match program.kind {
        ProgramKind::Module(module) => module.visit_with(&mut analyzer),
        ProgramKind::Script(script) => script.visit_with(&mut analyzer),
    }

    analyzer.diagnostics
}

struct ParsedProgram {
    kind: ProgramKind,
    file_start: BytePos,
}

enum ProgramKind {
    Module(Module),
    Script(swc_ecma_ast::Script),
}

fn parse_program(source: &str, is_ts: bool) -> Option<ParsedProgram> {
    let cm: Arc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom("svelte-script".into()).into(),
        source.to_string(),
    );
    let file_start = fm.start_pos;
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
    if let Ok(module) = parser.parse_module() {
        return Some(ParsedProgram {
            kind: ProgramKind::Module(module),
            file_start,
        });
    }

    let mut parser = Parser::new(syntax, StringInput::from(&*fm), None);
    let script = parser.parse_script().ok()?;
    Some(ParsedProgram {
        kind: ProgramKind::Script(script),
        file_start,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingKind {
    State,
    RawState,
    Derived,
    Prop,
    BindableProp,
    RestProp,
    Other,
}

#[derive(Debug, Clone)]
struct Binding {
    kind: BindingKind,
    function_depth: usize,
    initial: Option<Box<Expr>>,
    reassigned: bool,
}

#[derive(Debug, Clone)]
struct BindingInfo {
    ident: Ident,
    kind: BindingKind,
    initial: Option<Box<Expr>>,
}

#[derive(Debug, Default)]
struct Scope {
    bindings: FxHashMap<SmolStr, Binding>,
    is_function: bool,
}

struct StateReferenceAnalyzer {
    scopes: Vec<Scope>,
    function_depth: usize,
    state_call_depth: usize,
    diagnostics: Vec<Diagnostic>,
    file_start: BytePos,
    base_offset: u32,
    pending_function_name: Option<SmolStr>,
}

impl StateReferenceAnalyzer {
    fn new(file_start: BytePos, base_offset: u32) -> Self {
        Self {
            scopes: vec![Scope {
                bindings: FxHashMap::default(),
                is_function: true,
            }],
            function_depth: 0,
            state_call_depth: 0,
            diagnostics: Vec::new(),
            file_start,
            base_offset,
            pending_function_name: None,
        }
    }

    fn enter_scope(&mut self, is_function: bool) {
        self.scopes.push(Scope {
            bindings: FxHashMap::default(),
            is_function,
        });
        if is_function {
            self.function_depth += 1;
        }
    }

    fn exit_scope(&mut self, is_function: bool) {
        self.scopes.pop();
        if is_function {
            self.function_depth = self.function_depth.saturating_sub(1);
        }
    }

    fn current_scope_mut(&mut self) -> &mut Scope {
        self.scopes
            .last_mut()
            .expect("scope stack should not be empty")
    }

    fn resolve_binding(&self, name: &SmolStr) -> Option<&Binding> {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.bindings.get(name) {
                return Some(binding);
            }
        }
        None
    }

    fn resolve_binding_mut(&mut self, name: &SmolStr) -> Option<&mut Binding> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.bindings.contains_key(name) {
                return scope.bindings.get_mut(name);
            }
        }
        None
    }

    fn insert_binding(&mut self, name: SmolStr, binding: Binding, is_var: bool) {
        if is_var {
            for scope in self.scopes.iter_mut().rev() {
                if scope.is_function {
                    scope.bindings.insert(name, binding);
                    return;
                }
            }
        }
        self.current_scope_mut().bindings.insert(name, binding);
    }

    fn span_from_swc(&self, span: SwcSpan) -> SourceSpan {
        let start = span.lo.0.saturating_sub(self.file_start.0) + self.base_offset;
        let end = span.hi.0.saturating_sub(self.file_start.0) + self.base_offset;
        SourceSpan::new(start, end)
    }

    fn should_warn_for_state(&self, binding: &Binding) -> bool {
        if binding.reassigned {
            return true;
        }

        let Some(init) = &binding.initial else {
            return false;
        };

        let Expr::Call(call) = init.as_ref() else {
            return false;
        };

        if call.args.len() != 1 {
            return false;
        }

        let arg = &call.args[0];
        if arg.spread.is_some() {
            return false;
        }

        !self.should_proxy(&arg.expr, true)
    }

    fn should_proxy(&self, expr: &Expr, use_scope: bool) -> bool {
        match expr {
            Expr::Lit(_)
            | Expr::Tpl(_)
            | Expr::Arrow(_)
            | Expr::Fn(_)
            | Expr::Unary(_)
            | Expr::Bin(_) => false,
            Expr::Ident(ident) if ident.sym.as_ref() == "undefined" => false,
            Expr::Ident(ident) if use_scope => {
                let name = SmolStr::new(ident.sym.as_ref());
                if let Some(binding) = self.resolve_binding(&name) {
                    if !binding.reassigned {
                        if let Some(init) = &binding.initial {
                            return self.should_proxy(init, false);
                        }
                    }
                }
                true
            }
            _ => true,
        }
    }

    fn warn_if_needed(&mut self, ident: &Ident) {
        let name = SmolStr::new(ident.sym.as_ref());
        let Some(binding) = self.resolve_binding(&name) else {
            return;
        };

        let is_reactive = matches!(
            binding.kind,
            BindingKind::State | BindingKind::RawState | BindingKind::Derived | BindingKind::Prop
        );
        if !is_reactive {
            return;
        }

        if binding.function_depth != self.function_depth {
            return;
        }

        if matches!(binding.kind, BindingKind::State) && !self.should_warn_for_state(binding) {
            return;
        }

        let warning_type = if self.state_call_depth > 0 {
            "derived"
        } else {
            "closure"
        };

        let span = self.span_from_swc(ident.span);
        self.diagnostics.push(Diagnostic::new(
            DiagnosticCode::StateReferencedLocally,
            format!(
                "This reference only captures the initial value of `{}`. Did you mean to reference it inside a {} instead?",
                name, warning_type
            ),
            span,
        ));
    }

    fn handle_assignment_target(&mut self, target: &AssignTarget) {
        match target {
            AssignTarget::Simple(simple) => self.handle_simple_assign_target(simple),
            AssignTarget::Pat(pat) => self.handle_pat_assign_target(pat),
        }
    }

    fn handle_simple_assign_target(&mut self, target: &SimpleAssignTarget) {
        match target {
            SimpleAssignTarget::Ident(ident) => {
                let name = SmolStr::new(ident.sym.as_ref());
                if let Some(binding) = self.resolve_binding_mut(&name) {
                    binding.reassigned = true;
                }
            }
            SimpleAssignTarget::Member(member) => {
                member.obj.visit_with(self);
                if let MemberProp::Computed(computed) = &member.prop {
                    computed.expr.visit_with(self);
                }
            }
            SimpleAssignTarget::SuperProp(super_prop) => {
                if let SuperProp::Computed(computed) = &super_prop.prop {
                    computed.expr.visit_with(self);
                }
            }
            SimpleAssignTarget::Paren(expr) => {
                expr.expr.visit_with(self);
            }
            SimpleAssignTarget::OptChain(opt_chain) => {
                opt_chain.visit_with(self);
            }
            SimpleAssignTarget::TsAs(ts_as) => {
                ts_as.expr.visit_with(self);
            }
            SimpleAssignTarget::TsSatisfies(ts_sat) => {
                ts_sat.expr.visit_with(self);
            }
            SimpleAssignTarget::TsNonNull(ts_non) => {
                ts_non.expr.visit_with(self);
            }
            SimpleAssignTarget::TsTypeAssertion(ts_assert) => {
                ts_assert.expr.visit_with(self);
            }
            SimpleAssignTarget::TsInstantiation(ts_inst) => {
                ts_inst.expr.visit_with(self);
            }
            SimpleAssignTarget::Invalid(_) => {}
        }
    }

    fn handle_pat_assign_target(&mut self, pat: &AssignTargetPat) {
        match pat {
            AssignTargetPat::Array(arr) => {
                for elem in arr.elems.iter().flatten() {
                    self.mark_pat_reassigned(elem);
                    self.visit_pat(elem);
                }
            }
            AssignTargetPat::Object(obj) => {
                for prop in &obj.props {
                    match prop {
                        ObjectPatProp::KeyValue(kv) => {
                            self.mark_pat_reassigned(&kv.value);
                            self.visit_pat(&kv.value);
                        }
                        ObjectPatProp::Assign(assign) => {
                            if let Some(binding) =
                                self.resolve_binding_mut(&SmolStr::new(assign.key.id.sym.as_ref()))
                            {
                                binding.reassigned = true;
                            }
                            if let Some(value) = &assign.value {
                                value.visit_with(self);
                            }
                        }
                        ObjectPatProp::Rest(rest) => {
                            self.mark_pat_reassigned(&rest.arg);
                            self.visit_pat(&rest.arg);
                        }
                    }
                }
            }
            AssignTargetPat::Invalid(_) => {}
        }
    }

    fn mark_pat_reassigned(&mut self, pat: &Pat) {
        match pat {
            Pat::Ident(ident) => {
                let name = SmolStr::new(ident.id.sym.as_ref());
                if let Some(binding) = self.resolve_binding_mut(&name) {
                    binding.reassigned = true;
                }
            }
            Pat::Array(arr) => {
                for elem in arr.elems.iter().flatten() {
                    self.mark_pat_reassigned(elem);
                }
            }
            Pat::Object(obj) => {
                for prop in &obj.props {
                    match prop {
                        ObjectPatProp::KeyValue(kv) => self.mark_pat_reassigned(&kv.value),
                        ObjectPatProp::Assign(assign) => {
                            let name = SmolStr::new(assign.key.id.sym.as_ref());
                            if let Some(binding) = self.resolve_binding_mut(&name) {
                                binding.reassigned = true;
                            }
                        }
                        ObjectPatProp::Rest(rest) => self.mark_pat_reassigned(&rest.arg),
                    }
                }
            }
            Pat::Assign(assign) => {
                self.mark_pat_reassigned(&assign.left);
            }
            Pat::Rest(rest) => {
                self.mark_pat_reassigned(&rest.arg);
            }
            Pat::Expr(_) | Pat::Invalid(_) => {}
        }
    }
}

impl Visit for StateReferenceAnalyzer {
    fn visit_block_stmt(&mut self, block: &BlockStmt) {
        self.enter_scope(false);
        block.visit_children_with(self);
        self.exit_scope(false);
    }

    fn visit_fn_decl(&mut self, func: &FnDecl) {
        let name = SmolStr::new(func.ident.sym.as_ref());
        let binding = Binding {
            kind: BindingKind::Other,
            function_depth: self.function_depth,
            initial: None,
            reassigned: false,
        };
        self.insert_binding(name, binding, true);
        func.function.visit_with(self);
    }

    fn visit_fn_expr(&mut self, func: &FnExpr) {
        if let Some(ident) = &func.ident {
            self.pending_function_name = Some(SmolStr::new(ident.sym.as_ref()));
        }
        func.function.visit_with(self);
    }

    fn visit_arrow_expr(&mut self, func: &ArrowExpr) {
        self.enter_scope(true);
        self.declare_param_bindings_from_pats(&func.params);
        func.body.visit_with(self);
        self.exit_scope(true);
    }

    fn visit_function(&mut self, func: &Function) {
        self.enter_scope(true);
        if let Some(name) = self.pending_function_name.take() {
            let binding = Binding {
                kind: BindingKind::Other,
                function_depth: self.function_depth,
                initial: None,
                reassigned: false,
            };
            self.insert_binding(name, binding, false);
        }
        self.declare_param_bindings(&func.params);
        if let Some(body) = &func.body {
            body.visit_with(self);
        }
        self.exit_scope(true);
    }

    fn visit_var_decl(&mut self, decl: &VarDecl) {
        let is_var = decl.kind == VarDeclKind::Var;

        for declarator in &decl.decls {
            let init_kind = declarator.init.as_deref().and_then(rune_kind);

            match init_kind {
                Some(RuneKind::Props) => {
                    let mut bindings = Vec::new();
                    collect_props_bindings(&declarator.name, &mut bindings);
                    for info in bindings {
                        let binding = Binding {
                            kind: info.kind,
                            function_depth: self.function_depth,
                            initial: info.initial,
                            reassigned: false,
                        };
                        self.insert_binding(SmolStr::new(info.ident.sym.as_ref()), binding, is_var);
                    }
                }
                _ => {
                    let binding_kind = match init_kind {
                        Some(RuneKind::State) => BindingKind::State,
                        Some(RuneKind::RawState) => BindingKind::RawState,
                        Some(RuneKind::Derived) => BindingKind::Derived,
                        None => BindingKind::Other,
                        Some(RuneKind::Props) => BindingKind::Other,
                    };

                    let mut names = Vec::new();
                    collect_pat_idents(&declarator.name, &mut names);
                    for ident in &names {
                        let binding = Binding {
                            kind: binding_kind,
                            function_depth: self.function_depth,
                            initial: declarator.init.clone(),
                            reassigned: false,
                        };
                        self.insert_binding(SmolStr::new(ident.sym.as_ref()), binding, is_var);
                    }
                }
            }

            if let Some(init) = &declarator.init {
                if matches!(init_kind, Some(RuneKind::Props)) {
                    let depth = self.function_depth;
                    self.function_depth = depth + 1;
                    self.visit_pat(&declarator.name);
                    self.function_depth = depth;
                } else {
                    self.visit_pat(&declarator.name);
                }

                init.visit_with(self);
            } else {
                self.visit_pat(&declarator.name);
            }
        }
    }

    fn visit_assign_expr(&mut self, assign: &AssignExpr) {
        self.handle_assignment_target(&assign.left);
        assign.right.visit_with(self);
    }

    fn visit_update_expr(&mut self, update: &UpdateExpr) {
        match &*update.arg {
            Expr::Ident(ident) => {
                let name = SmolStr::new(ident.sym.as_ref());
                if let Some(binding) = self.resolve_binding_mut(&name) {
                    binding.reassigned = true;
                }
            }
            Expr::Member(member) => {
                member.obj.visit_with(self);
                if let MemberProp::Computed(computed) = &member.prop {
                    computed.expr.visit_with(self);
                }
            }
            _ => {
                update.arg.visit_with(self);
            }
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Ident(ident) => {
                self.warn_if_needed(ident);
            }
            _ => expr.visit_children_with(self),
        }
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        let call_rune = call_rune(call);
        let state_call = matches!(call_rune, Some(CallRune::State) | Some(CallRune::StateRaw));
        let derived_call = matches!(call_rune, Some(CallRune::Derived));
        let inspect_call = matches!(call_rune, Some(CallRune::Inspect));

        match &call.callee {
            Callee::Expr(expr) => {
                if !state_call {
                    expr.visit_with(self);
                }
            }
            Callee::Super(_) | Callee::Import(_) => {}
        }

        if state_call {
            self.state_call_depth += 1;
        }

        let prev_function_depth = self.function_depth;
        if derived_call || inspect_call {
            self.function_depth += 1;
        }

        for arg in &call.args {
            arg.expr.visit_with(self);
        }

        if derived_call || inspect_call {
            self.function_depth = prev_function_depth;
        }

        if state_call {
            self.state_call_depth = self.state_call_depth.saturating_sub(1);
        }
    }

    fn visit_prop(&mut self, prop: &Prop) {
        match prop {
            Prop::Shorthand(ident) => {
                self.warn_if_needed(ident);
            }
            _ => {
                prop.visit_children_with(self);
            }
        }
    }

    fn visit_getter_prop(&mut self, prop: &GetterProp) {
        prop.key.visit_with(self);
        self.enter_scope(true);
        if let Some(body) = &prop.body {
            body.visit_with(self);
        }
        self.exit_scope(true);
    }

    fn visit_setter_prop(&mut self, prop: &SetterProp) {
        prop.key.visit_with(self);
        self.enter_scope(true);
        let mut names = Vec::new();
        if let Some(this_param) = &prop.this_param {
            collect_pat_idents(this_param, &mut names);
        }
        collect_pat_idents(&prop.param, &mut names);
        for ident in names {
            let binding = Binding {
                kind: BindingKind::Other,
                function_depth: self.function_depth,
                initial: None,
                reassigned: false,
            };
            self.insert_binding(SmolStr::new(ident.sym.as_ref()), binding, false);
        }
        if let Some(body) = &prop.body {
            body.visit_with(self);
        }
        self.exit_scope(true);
    }

    fn visit_ts_property_signature(&mut self, _sig: &TsPropertySignature) {}
    fn visit_ts_method_signature(&mut self, _sig: &TsMethodSignature) {}
    fn visit_ts_getter_signature(&mut self, _sig: &TsGetterSignature) {}
    fn visit_ts_setter_signature(&mut self, _sig: &TsSetterSignature) {}
    fn visit_ts_index_signature(&mut self, _sig: &TsIndexSignature) {}

    fn visit_pat(&mut self, pat: &Pat) {
        match pat {
            Pat::Ident(_) => {}
            Pat::Array(arr) => {
                for elem in arr.elems.iter().flatten() {
                    self.visit_pat(elem);
                }
            }
            Pat::Object(obj) => {
                for prop in &obj.props {
                    match prop {
                        ObjectPatProp::KeyValue(kv) => {
                            self.visit_pat(&kv.value);
                        }
                        ObjectPatProp::Assign(assign) => {
                            if let Some(value) = &assign.value {
                                value.visit_with(self);
                            }
                        }
                        ObjectPatProp::Rest(rest) => {
                            self.visit_pat(&rest.arg);
                        }
                    }
                }
            }
            Pat::Assign(assign) => {
                self.visit_pat(&assign.left);
                assign.right.visit_with(self);
            }
            Pat::Rest(rest) => {
                self.visit_pat(&rest.arg);
            }
            Pat::Expr(expr) => {
                expr.visit_with(self);
            }
            Pat::Invalid(_) => {}
        }
    }

    fn visit_ts_type(&mut self, _ty: &TsType) {}
    fn visit_ts_type_ann(&mut self, _ann: &TsTypeAnn) {}
    fn visit_ts_type_param_instantiation(&mut self, _params: &TsTypeParamInstantiation) {}
    fn visit_ts_type_param_decl(&mut self, _decl: &TsTypeParamDecl) {}
}

impl StateReferenceAnalyzer {
    fn declare_param_bindings(&mut self, params: &[Param]) {
        let mut names = Vec::new();
        for param in params {
            collect_pat_idents(&param.pat, &mut names);
        }
        for ident in names {
            let binding = Binding {
                kind: BindingKind::Other,
                function_depth: self.function_depth,
                initial: None,
                reassigned: false,
            };
            self.insert_binding(SmolStr::new(ident.sym.as_ref()), binding, false);
        }
    }

    fn declare_param_bindings_from_pats(&mut self, params: &[Pat]) {
        let mut names = Vec::new();
        for pat in params {
            collect_pat_idents(pat, &mut names);
        }
        for ident in names {
            let binding = Binding {
                kind: BindingKind::Other,
                function_depth: self.function_depth,
                initial: None,
                reassigned: false,
            };
            self.insert_binding(SmolStr::new(ident.sym.as_ref()), binding, false);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuneKind {
    State,
    RawState,
    Derived,
    Props,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallRune {
    State,
    StateRaw,
    Derived,
    DerivedBy,
    Inspect,
}

fn rune_kind(expr: &Expr) -> Option<RuneKind> {
    let Expr::Call(call) = expr else {
        return None;
    };
    match &call.callee {
        Callee::Expr(callee) => match callee.as_ref() {
            Expr::Ident(ident) => match ident.sym.as_ref() {
                "$state" => Some(RuneKind::State),
                "$derived" => Some(RuneKind::Derived),
                "$props" => Some(RuneKind::Props),
                _ => None,
            },
            Expr::Member(member) => {
                if matches!(member.prop, MemberProp::Computed(_)) {
                    return None;
                }
                let obj_ident = match member.obj.as_ref() {
                    Expr::Ident(ident) => ident.sym.as_ref(),
                    _ => return None,
                };
                let prop_ident = match &member.prop {
                    MemberProp::Ident(ident) => ident.sym.as_ref(),
                    _ => return None,
                };
                match (obj_ident, prop_ident) {
                    ("$state", "raw") => Some(RuneKind::RawState),
                    ("$derived", "by") => Some(RuneKind::Derived),
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    }
}

fn call_rune(call: &CallExpr) -> Option<CallRune> {
    match &call.callee {
        Callee::Expr(expr) => match expr.as_ref() {
            Expr::Ident(ident) => match ident.sym.as_ref() {
                "$state" => Some(CallRune::State),
                "$derived" => Some(CallRune::Derived),
                "$inspect" => Some(CallRune::Inspect),
                _ => None,
            },
            Expr::Member(member) => {
                if matches!(member.prop, MemberProp::Computed(_)) {
                    return None;
                }
                match (member.obj.as_ref(), &member.prop) {
                    (Expr::Ident(obj), MemberProp::Ident(prop)) => {
                        match (obj.sym.as_ref(), prop.sym.as_ref()) {
                            ("$state", "raw") => Some(CallRune::StateRaw),
                            ("$derived", "by") => Some(CallRune::DerivedBy),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    }
}

fn collect_props_bindings(pat: &Pat, out: &mut Vec<BindingInfo>) {
    match pat {
        Pat::Ident(ident) => {
            out.push(BindingInfo {
                ident: ident.id.clone(),
                kind: BindingKind::RestProp,
                initial: None,
            });
        }
        Pat::Object(obj) => {
            for prop in &obj.props {
                match prop {
                    ObjectPatProp::Assign(assign) => {
                        let name = assign.key.id.clone();
                        let (kind, initial) = match assign.value.as_deref() {
                            Some(value) => bindable_or_prop(value),
                            None => (BindingKind::Prop, None),
                        };
                        out.push(BindingInfo {
                            ident: name,
                            kind,
                            initial,
                        });
                    }
                    ObjectPatProp::KeyValue(kv) => {
                        collect_props_bindings_from_pat(&kv.value, out);
                    }
                    ObjectPatProp::Rest(rest) => {
                        collect_props_bindings_with_kind(
                            &rest.arg,
                            BindingKind::RestProp,
                            None,
                            out,
                        );
                    }
                }
            }
        }
        Pat::Array(arr) => {
            for elem in arr.elems.iter().flatten() {
                collect_props_bindings(elem, out);
            }
        }
        Pat::Assign(assign) => {
            let (kind, initial) = bindable_or_prop(&assign.right);
            collect_props_bindings_with_kind(&assign.left, kind, initial, out);
        }
        Pat::Rest(rest) => {
            collect_props_bindings_with_kind(&rest.arg, BindingKind::RestProp, None, out);
        }
        Pat::Expr(_) | Pat::Invalid(_) => {}
    }
}

fn collect_props_bindings_from_pat(pat: &Pat, out: &mut Vec<BindingInfo>) {
    match pat {
        Pat::Assign(assign) => {
            let (kind, initial) = bindable_or_prop(&assign.right);
            collect_props_bindings_with_kind(&assign.left, kind, initial, out);
        }
        _ => collect_props_bindings_with_kind(pat, BindingKind::Prop, None, out),
    }
}

fn collect_props_bindings_with_kind(
    pat: &Pat,
    kind: BindingKind,
    initial: Option<Box<Expr>>,
    out: &mut Vec<BindingInfo>,
) {
    match pat {
        Pat::Ident(ident) => {
            out.push(BindingInfo {
                ident: ident.id.clone(),
                kind,
                initial,
            });
        }
        Pat::Array(arr) => {
            for elem in arr.elems.iter().flatten() {
                collect_props_bindings_with_kind(elem, kind, initial.clone(), out);
            }
        }
        Pat::Object(obj) => {
            for prop in &obj.props {
                match prop {
                    ObjectPatProp::KeyValue(kv) => {
                        collect_props_bindings_with_kind(&kv.value, kind, initial.clone(), out);
                    }
                    ObjectPatProp::Assign(assign) => {
                        out.push(BindingInfo {
                            ident: assign.key.id.clone(),
                            kind,
                            initial: assign.value.as_deref().map(|value| value.clone().into()),
                        });
                    }
                    ObjectPatProp::Rest(rest) => {
                        collect_props_bindings_with_kind(&rest.arg, kind, initial.clone(), out);
                    }
                }
            }
        }
        Pat::Assign(assign) => {
            collect_props_bindings_with_kind(&assign.left, kind, initial.clone(), out);
        }
        Pat::Rest(rest) => {
            collect_props_bindings_with_kind(&rest.arg, kind, initial, out);
        }
        Pat::Expr(_) | Pat::Invalid(_) => {}
    }
}

fn bindable_or_prop(expr: &Expr) -> (BindingKind, Option<Box<Expr>>) {
    if let Some(initial) = bindable_initial(expr) {
        return (BindingKind::BindableProp, initial);
    }
    (BindingKind::Prop, Some(expr.clone().into()))
}

fn bindable_initial(expr: &Expr) -> Option<Option<Box<Expr>>> {
    let Expr::Call(call) = expr else {
        return None;
    };
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Ident(ident) = callee.as_ref() else {
        return None;
    };
    if ident.sym.as_ref() != "$bindable" {
        return None;
    }
    match call.args.len() {
        0 => Some(None),
        1 => {
            let arg = &call.args[0];
            if arg.spread.is_some() {
                return Some(None);
            }
            Some(Some(arg.expr.clone()))
        }
        _ => Some(None),
    }
}

fn collect_pat_idents(pat: &Pat, out: &mut Vec<Ident>) {
    match pat {
        Pat::Ident(ident) => out.push(ident.id.clone()),
        Pat::Array(arr) => {
            for elem in arr.elems.iter().flatten() {
                collect_pat_idents(elem, out);
            }
        }
        Pat::Object(obj) => {
            for prop in &obj.props {
                match prop {
                    ObjectPatProp::KeyValue(kv) => collect_pat_idents(&kv.value, out),
                    ObjectPatProp::Assign(assign) => out.push(assign.key.id.clone()),
                    ObjectPatProp::Rest(rest) => collect_pat_idents(&rest.arg, out),
                }
            }
        }
        Pat::Assign(assign) => collect_pat_idents(&assign.left, out),
        Pat::Rest(rest) => collect_pat_idents(&rest.arg, out),
        Pat::Expr(_) | Pat::Invalid(_) => {}
    }
}
