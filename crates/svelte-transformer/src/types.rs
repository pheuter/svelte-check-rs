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
    /// For Svelte 5, components are functions, not classes. We export a const
    /// with the Component type to enable proper type inference.
    ///
    /// Uses a unique internal name `__SvelteComponent_{name}_` to avoid conflicts
    /// with user imports that might have the same name (e.g., importing `Page`
    /// while also being in a `+page.svelte` file).
    ///
    /// Produces output like:
    /// ```text
    /// declare const __SvelteComponent_Counter_: Component<Props>;
    /// export default __SvelteComponent_Counter_;
    /// ```
    pub fn generate_typescript_export(&self, component_name: &str) -> String {
        let internal_name = format!("__SvelteComponent_{}_", component_name);
        format!(
            "declare const {}: Component<{}>;\nexport default {};\n",
            internal_name,
            self.props_or_default(),
            internal_name
        )
    }

    /// Generates the component export line for a JavaScript component.
    ///
    /// Uses a unique internal name to avoid conflicts with user imports.
    ///
    /// Produces output like:
    /// ```text
    /// declare const __SvelteComponent_Counter_: Component<{}>;
    /// export default __SvelteComponent_Counter_;
    /// ```
    pub fn generate_javascript_export(&self, component_name: &str) -> String {
        let internal_name = format!("__SvelteComponent_{}_", component_name);
        format!(
            "declare const {}: Component<{{}}>;\nexport default {};\n",
            internal_name, internal_name
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
/// Sanitizes the name to be a valid TypeScript identifier.
pub fn component_name_from_path(path: &str) -> String {
    let name = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Component".to_string());

    sanitize_component_name(&name)
}

/// Sanitizes a component name to be a valid TypeScript identifier.
/// - Removes leading invalid characters (like `+` in `+page.svelte`)
/// - Converts to PascalCase
/// - Replaces invalid characters with underscores
fn sanitize_component_name(name: &str) -> String {
    // Skip leading non-alphabetic characters (like `+` in SvelteKit files)
    let name = name.trim_start_matches(|c: char| !c.is_alphabetic());

    if name.is_empty() {
        return "Component".to_string();
    }

    // Convert to PascalCase and remove invalid characters
    let mut result = String::with_capacity(name.len());
    let mut capitalize_next = true;

    for c in name.chars() {
        if c.is_alphanumeric() || c == '_' {
            if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(c);
            }
        } else if c == '-' || c == '.' {
            // These trigger capitalization of the next character
            capitalize_next = true;
        }
        // Other characters are skipped
    }

    if result.is_empty() {
        "Component".to_string()
    } else {
        result
    }
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
        // Uses internal name to avoid conflicts with imports
        assert!(export_line.contains("declare const __SvelteComponent_Counter_: Component<"));
        assert!(export_line.contains("{ count: number }"));
        assert!(export_line.contains("export default __SvelteComponent_Counter_"));
    }

    #[test]
    fn test_generate_javascript_export() {
        let exports = ComponentExports::new();
        let export_line = exports.generate_javascript_export("Button");
        // Uses internal name to avoid conflicts with imports
        assert!(export_line.contains("declare const __SvelteComponent_Button_: Component<{}>"));
        assert!(export_line.contains("export default __SvelteComponent_Button_"));
    }

    #[test]
    fn test_component_name_from_path() {
        assert_eq!(component_name_from_path("Counter.svelte"), "Counter");
        assert_eq!(
            component_name_from_path("/path/to/MyComponent.svelte"),
            "MyComponent"
        );
        assert_eq!(component_name_from_path(""), "Component");

        // SvelteKit special files
        assert_eq!(component_name_from_path("+page.svelte"), "Page");
        assert_eq!(component_name_from_path("+layout.svelte"), "Layout");
        assert_eq!(component_name_from_path("+error.svelte"), "Error");
        assert_eq!(component_name_from_path("+page.server.ts"), "PageServer");
    }

    #[test]
    fn test_sanitize_component_name() {
        assert_eq!(sanitize_component_name("Counter"), "Counter");
        assert_eq!(sanitize_component_name("+page"), "Page");
        assert_eq!(sanitize_component_name("+layout"), "Layout");
        assert_eq!(sanitize_component_name("my-component"), "MyComponent");
        assert_eq!(sanitize_component_name("+++"), "Component");
        assert_eq!(sanitize_component_name(""), "Component");
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
