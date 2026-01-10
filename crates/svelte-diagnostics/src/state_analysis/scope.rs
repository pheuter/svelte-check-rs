//! Scope and binding tracking for state analysis.

use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use swc_ecma_ast::Id;

/// The kind of binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    /// A `$state()` binding.
    State,
    /// A `$state.raw()` binding.
    StateRaw,
    /// A `$derived()` or `$derived.by()` binding.
    Derived,
    /// A `$props()` binding (the whole destructured object).
    Props,
    /// A binding destructured from `$props()`.
    PropMember,
    /// A `$bindable()` binding.
    Bindable,
    /// A regular variable (not reactive).
    Regular,
}

impl BindingKind {
    /// Returns true if this binding kind represents reactive state.
    pub fn is_reactive(&self) -> bool {
        matches!(
            self,
            BindingKind::State
                | BindingKind::StateRaw
                | BindingKind::Derived
                | BindingKind::Props
                | BindingKind::PropMember
                | BindingKind::Bindable
        )
    }
}

/// A variable binding.
#[derive(Debug, Clone)]
pub struct Binding {
    /// The binding's name.
    pub name: SmolStr,
    /// The kind of binding.
    pub kind: BindingKind,
    /// The function depth at which the binding was declared.
    pub function_depth: usize,
    /// Whether the binding has been reassigned.
    pub reassigned: bool,
    /// The byte offset of the binding declaration.
    pub decl_offset: u32,
}

/// A scope for tracking variable bindings.
#[derive(Debug, Default)]
pub struct Scope {
    /// Bindings in this scope, keyed by SWC identifier.
    bindings: FxHashMap<Id, Binding>,
    /// The current function depth (0 = top-level script).
    function_depth: usize,
}

impl Scope {
    /// Creates a new scope.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current function depth.
    pub fn function_depth(&self) -> usize {
        self.function_depth
    }

    /// Increments the function depth (entering a function/arrow).
    pub fn enter_function(&mut self) {
        self.function_depth += 1;
    }

    /// Decrements the function depth (leaving a function/arrow).
    pub fn leave_function(&mut self) {
        self.function_depth = self.function_depth.saturating_sub(1);
    }

    /// Adds a binding to the scope.
    pub fn add_binding(&mut self, id: Id, name: SmolStr, kind: BindingKind, offset: u32) {
        self.bindings.insert(
            id,
            Binding {
                name,
                kind,
                function_depth: self.function_depth,
                reassigned: false,
                decl_offset: offset,
            },
        );
    }

    /// Gets a binding by its SWC identifier.
    pub fn get_binding(&self, id: &Id) -> Option<&Binding> {
        self.bindings.get(id)
    }

    /// Marks a binding as reassigned.
    pub fn mark_reassigned(&mut self, id: &Id) {
        if let Some(binding) = self.bindings.get_mut(id) {
            binding.reassigned = true;
        }
    }
}
