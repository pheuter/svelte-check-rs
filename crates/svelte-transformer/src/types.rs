//! Type definitions for transformer output.

/// Exported types from a Svelte component.
#[derive(Debug, Clone, Default)]
pub struct ComponentExports {
    /// The props type (extracted from `$props()`).
    pub props_type: Option<String>,
    /// The events type.
    pub events_type: Option<String>,
    /// The slots type.
    pub slots_type: Option<String>,
}
