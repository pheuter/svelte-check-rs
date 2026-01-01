//! Type definitions for transformer output.

use std::path::Path;

/// Exported types from a Svelte component.
#[derive(Debug, Clone, Default)]
pub struct ComponentExports {
    /// The props type (extracted from `$props()`).
    pub props_type: Option<String>,
    /// The events type.
    pub events_type: Option<String>,
    /// The slots type.
    pub slots_type: Option<String>,
    /// Bindable prop names (for two-way binding support).
    pub bindable_props: Vec<String>,
}

impl ComponentExports {
    /// Creates a new empty ComponentExports.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the props type or a default empty object type.
    pub fn props_or_default(&self) -> &str {
        self.props_type.as_deref().unwrap_or("{}")
    }

    /// Returns the events type or a default empty object type.
    pub fn events_or_default(&self) -> &str {
        self.events_type.as_deref().unwrap_or("{}")
    }

    /// Returns the slots type or a default empty object type.
    pub fn slots_or_default(&self) -> &str {
        self.slots_type.as_deref().unwrap_or("{}")
    }

    /// Generates the component export line for a TypeScript component.
    ///
    /// Produces output like:
    /// ```text
    /// export default class Counter extends SvelteComponent<Props, Events, Slots> {}
    /// ```
    pub fn generate_typescript_export(&self, component_name: &str) -> String {
        format!(
            "export default class {} extends SvelteComponent<{}, {}, {}> {{}}\n",
            component_name,
            self.props_or_default(),
            self.events_or_default(),
            self.slots_or_default()
        )
    }

    /// Generates the component export line for a JavaScript component.
    ///
    /// Produces output like:
    /// ```text
    /// export default class Counter extends SvelteComponent {}
    /// ```
    pub fn generate_javascript_export(&self, component_name: &str) -> String {
        format!(
            "export default class {} extends SvelteComponent {{}}\n",
            component_name
        )
    }

    /// Generates the appropriate export based on whether TypeScript is used.
    pub fn generate_export(&self, component_name: &str, is_typescript: bool) -> String {
        if is_typescript {
            self.generate_typescript_export(component_name)
        } else {
            self.generate_javascript_export(component_name)
        }
    }
}

/// Extracts a component name from a filename.
///
/// Given a path like `/path/to/Counter.svelte`, returns `"Counter"`.
/// Returns `"Component"` if no valid name can be extracted.
pub fn component_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Component".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_exports() {
        let exports = ComponentExports::new();
        assert_eq!(exports.props_or_default(), "{}");
        assert_eq!(exports.events_or_default(), "{}");
        assert_eq!(exports.slots_or_default(), "{}");
    }

    #[test]
    fn test_with_props_type() {
        let exports = ComponentExports {
            props_type: Some("{ count: number }".to_string()),
            ..Default::default()
        };
        assert_eq!(exports.props_or_default(), "{ count: number }");
    }

    #[test]
    fn test_generate_typescript_export() {
        let exports = ComponentExports {
            props_type: Some("{ count: number }".to_string()),
            ..Default::default()
        };
        let export_line = exports.generate_typescript_export("Counter");
        assert!(export_line.contains("class Counter extends SvelteComponent"));
        assert!(export_line.contains("{ count: number }"));
    }

    #[test]
    fn test_generate_javascript_export() {
        let exports = ComponentExports::new();
        let export_line = exports.generate_javascript_export("Button");
        assert_eq!(
            export_line,
            "export default class Button extends SvelteComponent {}\n"
        );
    }

    #[test]
    fn test_component_name_from_path() {
        assert_eq!(component_name_from_path("Counter.svelte"), "Counter");
        assert_eq!(
            component_name_from_path("/path/to/MyComponent.svelte"),
            "MyComponent"
        );
        assert_eq!(component_name_from_path(""), "Component");
    }

    #[test]
    fn test_bindable_props() {
        let exports = ComponentExports {
            props_type: Some("{ value: string }".to_string()),
            bindable_props: vec!["value".to_string()],
            ..Default::default()
        };
        assert!(exports.bindable_props.contains(&"value".to_string()));
    }
}
