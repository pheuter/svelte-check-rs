//! AST visitor for state reference analysis.

use super::scope::{Binding, BindingKind, Scope};
use super::create_diagnostic;
use crate::Diagnostic;
use smol_str::SmolStr;
use source_map::Span;
use swc_ecma_ast::*;
use swc_ecma_visit::{Visit, VisitWith};

/// Analyzer for detecting state_referenced_locally warnings.
pub struct StateAnalyzer {
    /// The scope tracker.
    scope: Scope,
    /// Collected diagnostics.
    diagnostics: Vec<Diagnostic>,
    /// The span of the script content (for offset mapping).
    content_span: Span,
    /// Whether we're currently in an assignment LHS.
    in_assignment_lhs: bool,
    /// Whether we're currently in an update expression.
    in_update: bool,
}

impl StateAnalyzer {
    /// Creates a new analyzer.
    pub fn new(content_span: Span) -> Self {
        Self {
            scope: Scope::new(),
            diagnostics: Vec::new(),
            content_span,
            in_assignment_lhs: false,
            in_update: false,
        }
    }

    /// Analyzes a module for state_referenced_locally warnings.
    pub fn analyze(&mut self, module: &Module) {
        // First pass: collect all bindings
        self.collect_bindings(module);

        // Second pass: check for problematic references
        self.check_references(module);
    }

    /// Returns the collected diagnostics.
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Converts a SWC byte position to a source span.
    fn to_span(&self, lo: u32, hi: u32) -> Span {
        Span::new(
            self.content_span.start + lo,
            self.content_span.start + hi,
        )
    }

    /// Collects all bindings from the module.
    fn collect_bindings(&mut self, module: &Module) {
        for item in &module.body {
            if let ModuleItem::Stmt(stmt) = item {
                self.collect_stmt_bindings(stmt);
            }
        }
    }

    /// Collects bindings from a statement.
    fn collect_stmt_bindings(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Decl(Decl::Var(var_decl)) => {
                for decl in &var_decl.decls {
                    self.collect_var_decl_binding(decl);
                }
            }
            Stmt::Block(block) => {
                for stmt in &block.stmts {
                    self.collect_stmt_bindings(stmt);
                }
            }
            Stmt::If(if_stmt) => {
                self.collect_stmt_bindings(&if_stmt.cons);
                if let Some(alt) = &if_stmt.alt {
                    self.collect_stmt_bindings(alt);
                }
            }
            _ => {}
        }
    }

    /// Collects a binding from a variable declarator.
    fn collect_var_decl_binding(&mut self, decl: &VarDeclarator) {
        let kind = decl
            .init
            .as_ref()
            .map(|init| self.get_rune_kind(init))
            .unwrap_or(BindingKind::Regular);

        match &decl.name {
            Pat::Ident(ident) => {
                let offset = ident.span.lo.0;
                self.scope.add_binding(
                    ident.to_id(),
                    SmolStr::new(&ident.sym),
                    kind,
                    offset,
                );
            }
            Pat::Object(obj_pat) => {
                // Handle destructuring from $props()
                if kind == BindingKind::Props {
                    self.collect_props_destructuring(obj_pat);
                }
            }
            _ => {}
        }
    }

    /// Collects bindings from props destructuring pattern.
    fn collect_props_destructuring(&mut self, obj_pat: &ObjectPat) {
        for prop in &obj_pat.props {
            match prop {
                ObjectPatProp::KeyValue(kv) => {
                    if let Pat::Ident(ident) = &*kv.value {
                        let offset = ident.span.lo.0;
                        self.scope.add_binding(
                            ident.to_id(),
                            SmolStr::new(&ident.sym),
                            BindingKind::PropMember,
                            offset,
                        );
                    }
                }
                ObjectPatProp::Assign(assign) => {
                    let offset = assign.span.lo.0;
                    self.scope.add_binding(
                        assign.key.to_id(),
                        SmolStr::new(&assign.key.sym),
                        BindingKind::PropMember,
                        offset,
                    );
                }
                ObjectPatProp::Rest(rest) => {
                    if let Pat::Ident(ident) = &*rest.arg {
                        let offset = ident.span.lo.0;
                        self.scope.add_binding(
                            ident.to_id(),
                            SmolStr::new(&ident.sym),
                            BindingKind::PropMember,
                            offset,
                        );
                    }
                }
            }
        }
    }

    /// Determines the binding kind from a rune call.
    fn get_rune_kind(&self, expr: &Expr) -> BindingKind {
        match expr {
            Expr::Call(call) => {
                if let Some(name) = self.get_callee_name(call) {
                    match name.as_str() {
                        "$state" => BindingKind::State,
                        "$state.raw" => BindingKind::StateRaw,
                        "$derived" | "$derived.by" => BindingKind::Derived,
                        "$props" => BindingKind::Props,
                        "$bindable" => BindingKind::Bindable,
                        _ => BindingKind::Regular,
                    }
                } else {
                    BindingKind::Regular
                }
            }
            _ => BindingKind::Regular,
        }
    }

    /// Gets the callee name from a call expression (handles member expressions).
    fn get_callee_name(&self, call: &CallExpr) -> Option<String> {
        match &call.callee {
            Callee::Expr(expr) => match &**expr {
                Expr::Ident(ident) => Some(ident.sym.to_string()),
                Expr::Member(member) => {
                    if let (Expr::Ident(obj), MemberProp::Ident(prop)) =
                        (&*member.obj, &member.prop)
                    {
                        Some(format!("{}.{}", obj.sym, prop.sym))
                    } else {
                        None
                    }
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Checks all references in the module.
    fn check_references(&mut self, module: &Module) {
        module.visit_with(self);
    }

    /// Checks if an identifier reference is problematic.
    fn check_identifier_reference(&mut self, ident: &Ident) {
        // Skip if we're in assignment LHS or update expression
        if self.in_assignment_lhs || self.in_update {
            return;
        }

        let binding = match self.scope.get_binding(&ident.to_id()) {
            Some(b) => b,
            None => return,
        };

        // Only warn for reactive bindings
        if !binding.kind.is_reactive() {
            return;
        }

        // Only warn if at the same function depth (not inside a closure)
        if self.scope.function_depth() != binding.function_depth {
            return;
        }

        // Don't warn if this is the declaration itself
        if ident.span.lo.0 == binding.decl_offset {
            return;
        }

        // Determine suggestion type
        let suggestion_type = match binding.kind {
            BindingKind::State | BindingKind::StateRaw => "derived",
            _ => "closure",
        };

        let span = self.to_span(ident.span.lo.0, ident.span.hi.0);
        self.diagnostics
            .push(create_diagnostic(&binding.name, span, suggestion_type));
    }
}

impl Visit for StateAnalyzer {
    fn visit_ident(&mut self, ident: &Ident) {
        self.check_identifier_reference(ident);
    }

    fn visit_function(&mut self, func: &Function) {
        self.scope.enter_function();
        func.visit_children_with(self);
        self.scope.leave_function();
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        self.scope.enter_function();
        arrow.visit_children_with(self);
        self.scope.leave_function();
    }

    fn visit_assign_expr(&mut self, expr: &AssignExpr) {
        // Visit RHS first (normal)
        expr.right.visit_with(self);

        // Visit LHS with flag set
        self.in_assignment_lhs = true;
        expr.left.visit_with(self);
        self.in_assignment_lhs = false;
    }

    fn visit_update_expr(&mut self, expr: &UpdateExpr) {
        self.in_update = true;
        expr.visit_children_with(self);
        self.in_update = false;
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        // Check if this is a $state() or $state.raw() call - don't warn for arguments
        if let Some(name) = self.get_callee_name(call) {
            if name == "$state" || name == "$state.raw" {
                // Skip visiting arguments - these are initialization values
                return;
            }

            // For $derived, $effect, etc., the function argument creates a closure
            if name.starts_with("$derived")
                || name.starts_with("$effect")
                || name.starts_with("$inspect")
            {
                // Visit callee normally
                call.callee.visit_with(self);
                // Visit arguments with increased function depth
                self.scope.enter_function();
                for arg in &call.args {
                    arg.visit_with(self);
                }
                self.scope.leave_function();
                return;
            }
        }

        // Normal call - visit everything
        call.visit_children_with(self);
    }

    fn visit_var_declarator(&mut self, decl: &VarDeclarator) {
        // Skip visiting the name pattern (it's the declaration)
        // Only visit the init expression
        if let Some(init) = &decl.init {
            init.visit_with(self);
        }
    }
}
