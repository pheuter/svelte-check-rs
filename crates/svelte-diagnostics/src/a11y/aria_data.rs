//! ARIA data tables for a11y validation.
//!
//! Reference: WAI-ARIA 1.2 specification

/// Valid ARIA attribute names.
pub static ARIA_ATTRIBUTES: &[&str] = &[
    "aria-activedescendant",
    "aria-atomic",
    "aria-autocomplete",
    "aria-braillelabel",
    "aria-brailleroledescription",
    "aria-busy",
    "aria-checked",
    "aria-colcount",
    "aria-colindex",
    "aria-colindextext",
    "aria-colspan",
    "aria-controls",
    "aria-current",
    "aria-describedby",
    "aria-description",
    "aria-details",
    "aria-disabled",
    "aria-dropeffect",
    "aria-errormessage",
    "aria-expanded",
    "aria-flowto",
    "aria-grabbed",
    "aria-haspopup",
    "aria-hidden",
    "aria-invalid",
    "aria-keyshortcuts",
    "aria-label",
    "aria-labelledby",
    "aria-level",
    "aria-live",
    "aria-modal",
    "aria-multiline",
    "aria-multiselectable",
    "aria-orientation",
    "aria-owns",
    "aria-placeholder",
    "aria-posinset",
    "aria-pressed",
    "aria-readonly",
    "aria-relevant",
    "aria-required",
    "aria-roledescription",
    "aria-rowcount",
    "aria-rowindex",
    "aria-rowindextext",
    "aria-rowspan",
    "aria-selected",
    "aria-setsize",
    "aria-sort",
    "aria-valuemax",
    "aria-valuemin",
    "aria-valuenow",
    "aria-valuetext",
];

/// Check if an attribute name is a valid ARIA attribute.
pub fn is_valid_aria_attribute(name: &str) -> bool {
    ARIA_ATTRIBUTES.contains(&name)
}

/// Elements that are inherently interactive.
pub static INTERACTIVE_ELEMENTS: &[&str] = &[
    "a", "button", "input", "select", "textarea", "details", "embed", "iframe", "menu", "summary",
];

/// Check if an element is interactive by default.
pub fn is_interactive_element(tag: &str) -> bool {
    INTERACTIVE_ELEMENTS.contains(&tag)
}

/// Elements that are non-interactive by default.
pub static NON_INTERACTIVE_ELEMENTS: &[&str] = &[
    "div",
    "span",
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "section",
    "article",
    "main",
    "aside",
    "header",
    "footer",
    "nav",
    "figure",
    "figcaption",
    "blockquote",
    "pre",
    "code",
    "ul",
    "ol",
    "li",
    "dl",
    "dt",
    "dd",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "th",
    "td",
    "caption",
    "form",
    "fieldset",
    "legend",
    "label",
    "hr",
    "br",
    "img",
    "picture",
    "video",
    "audio",
    "canvas",
    "svg",
    "address",
    "time",
    "abbr",
    "cite",
    "em",
    "strong",
    "small",
    "sub",
    "sup",
    "mark",
    "del",
    "ins",
    "s",
    "u",
    "b",
    "i",
];

/// Check if an element is non-interactive by default.
pub fn is_non_interactive_element(tag: &str) -> bool {
    NON_INTERACTIVE_ELEMENTS.contains(&tag)
}

/// Roles that are interactive.
/// Reference: WAI-ARIA 1.2 - https://www.w3.org/TR/wai-aria-1.2/
pub static INTERACTIVE_ROLES: &[&str] = &[
    "application", // Declares a region as a web application (interactive widget)
    "button",
    "checkbox",
    "combobox",
    "gridcell",
    "link",
    "listbox",
    "menu",
    "menubar",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "progressbar",
    "radio",
    "scrollbar",
    "searchbox",
    "slider",
    "spinbutton",
    "switch",
    "tab",
    "tabpanel",
    "textbox",
    "treeitem",
];

/// Check if a role is interactive.
pub fn is_interactive_role(role: &str) -> bool {
    INTERACTIVE_ROLES.contains(&role)
}

/// Non-interactive roles.
pub static NON_INTERACTIVE_ROLES: &[&str] = &[
    "alert",
    "alertdialog",
    "article",
    "banner",
    "blockquote",
    "caption",
    "cell",
    "code",
    "columnheader",
    "complementary",
    "contentinfo",
    "definition",
    "deletion",
    "dialog",
    "directory",
    "document",
    "emphasis",
    "feed",
    "figure",
    "form",
    "generic",
    "grid",
    "group",
    "heading",
    "img",
    "insertion",
    "list",
    "listitem",
    "log",
    "main",
    "marquee",
    "math",
    "meter",
    "navigation",
    "note",
    "paragraph",
    "presentation",
    "none",
    "region",
    "row",
    "rowgroup",
    "rowheader",
    "search",
    "separator",
    "status",
    "strong",
    "subscript",
    "superscript",
    "table",
    "term",
    "time",
    "timer",
    "toolbar",
    "tooltip",
    "tree",
    "treegrid",
];

/// Get required ARIA properties for a role.
pub fn get_required_aria_props(role: &str) -> &'static [&'static str] {
    match role {
        "checkbox" => &["aria-checked"],
        "combobox" => &["aria-controls", "aria-expanded"],
        "heading" => &["aria-level"],
        "meter" => &["aria-valuenow"],
        "option" => &["aria-selected"],
        "radio" => &["aria-checked"],
        "scrollbar" => &[
            "aria-controls",
            "aria-valuenow",
            "aria-valuemax",
            "aria-valuemin",
        ],
        "separator" => &["aria-valuenow", "aria-valuemax", "aria-valuemin"],
        "slider" => &["aria-valuenow"],
        "spinbutton" => &["aria-valuenow"],
        "switch" => &["aria-checked"],
        _ => &[],
    }
}

/// Get the implicit role for an element.
pub fn get_implicit_role(tag: &str) -> Option<&'static str> {
    match tag {
        "a" => Some("link"),
        "article" => Some("article"),
        "aside" => Some("complementary"),
        "button" => Some("button"),
        "dialog" => Some("dialog"),
        "footer" => Some("contentinfo"),
        "form" => Some("form"),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Some("heading"),
        "header" => Some("banner"),
        "hr" => Some("separator"),
        "img" => Some("img"),
        "input" => Some("textbox"), // Simplified, depends on type
        "li" => Some("listitem"),
        "main" => Some("main"),
        "menu" => Some("list"),
        "nav" => Some("navigation"),
        "ol" | "ul" => Some("list"),
        "option" => Some("option"),
        "progress" => Some("progressbar"),
        "section" => Some("region"),
        "select" => Some("combobox"), // Simplified
        "table" => Some("table"),
        "tbody" | "tfoot" | "thead" => Some("rowgroup"),
        "td" => Some("cell"),
        "textarea" => Some("textbox"),
        "th" => Some("columnheader"),
        "tr" => Some("row"),
        _ => None,
    }
}

/// Check if a role is redundant for an element (element already has that role implicitly).
pub fn is_redundant_role(tag: &str, role: &str) -> bool {
    matches!(
        (tag, role),
        ("a", "link")
            | ("article", "article")
            | ("aside", "complementary")
            | ("button", "button")
            | ("dialog", "dialog")
            | ("form", "form")
            | ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", "heading")
            | ("hr", "separator")
            | ("img", "img")
            | ("li", "listitem")
            | ("main", "main")
            | ("nav", "navigation")
            | ("ol" | "ul", "list")
            | ("option", "option")
            | ("progress", "progressbar")
            | ("section", "region")
            | ("table", "table")
            | ("tbody" | "tfoot" | "thead", "rowgroup")
            | ("td", "cell")
            | ("th", "columnheader")
            | ("tr", "row")
    )
}

/// Check if an element has input event handlers (onclick, etc.).
pub fn has_click_handler(attrs: &[(String, Option<String>)]) -> bool {
    attrs.iter().any(|(name, _)| {
        matches!(
            name.as_str(),
            "onclick" | "on:click" | "ondblclick" | "on:dblclick"
        )
    })
}

/// Check if an element has keyboard event handlers.
pub fn has_key_handler(attrs: &[(String, Option<String>)]) -> bool {
    attrs.iter().any(|(name, _)| {
        matches!(
            name.as_str(),
            "onkeydown" | "on:keydown" | "onkeyup" | "on:keyup" | "onkeypress" | "on:keypress"
        )
    })
}

/// Check if an element has mouse event handlers.
pub fn has_mouse_handler(attrs: &[(String, Option<String>)]) -> bool {
    attrs.iter().any(|(name, _)| {
        matches!(
            name.as_str(),
            "onmouseover" | "on:mouseover" | "onmouseout" | "on:mouseout"
        )
    })
}

/// Check if an element has focus event handlers.
pub fn has_focus_handler(attrs: &[(String, Option<String>)]) -> bool {
    attrs
        .iter()
        .any(|(name, _)| matches!(name.as_str(), "onfocus" | "on:focus" | "onblur" | "on:blur"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_aria_attribute() {
        assert!(is_valid_aria_attribute("aria-label"));
        assert!(is_valid_aria_attribute("aria-hidden"));
        assert!(!is_valid_aria_attribute("aria-invalid-attr"));
        assert!(!is_valid_aria_attribute("aria-foo"));
    }

    #[test]
    fn test_interactive_elements() {
        assert!(is_interactive_element("button"));
        assert!(is_interactive_element("a"));
        assert!(is_interactive_element("input"));
        assert!(!is_interactive_element("div"));
        assert!(!is_interactive_element("span"));
    }

    #[test]
    fn test_interactive_roles() {
        assert!(is_interactive_role("button"));
        assert!(is_interactive_role("link"));
        assert!(is_interactive_role("textbox"));
        assert!(is_interactive_role("application"));
        assert!(!is_interactive_role("document"));
        assert!(!is_interactive_role("article"));
    }

    #[test]
    fn test_required_aria_props() {
        assert_eq!(get_required_aria_props("checkbox"), &["aria-checked"]);
        assert_eq!(get_required_aria_props("slider"), &["aria-valuenow"]);
        assert!(get_required_aria_props("button").is_empty());
    }

    #[test]
    fn test_redundant_roles() {
        assert!(is_redundant_role("button", "button"));
        assert!(is_redundant_role("a", "link"));
        assert!(!is_redundant_role("div", "button"));
        assert!(!is_redundant_role("a", "button"));
    }
}
