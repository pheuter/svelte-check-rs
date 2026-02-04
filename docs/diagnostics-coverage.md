# Diagnostics Coverage

This report compares Svelte compiler warning codes (underscore style) against
svelte-check-rs internal diagnostics (kebab style).

<!-- COVERAGE:START -->
## Summary
- Compiler warnings: 81
- Internal diagnostics: 4
- Overlap: 0
- Compiler-only: 81
- Internal-only: 4

## Overlap
- (none)

## Compiler-only
- a11y_accesskey
- a11y_aria_activedescendant_has_tabindex
- a11y_aria_attributes
- a11y_autocomplete_valid
- a11y_autofocus
- a11y_click_events_have_key_events
- a11y_consider_explicit_label
- a11y_distracting_elements
- a11y_figcaption_index
- a11y_figcaption_parent
- a11y_hidden
- a11y_img_redundant_alt
- a11y_incorrect_aria_attribute_type
- a11y_incorrect_aria_attribute_type_boolean
- a11y_incorrect_aria_attribute_type_id
- a11y_incorrect_aria_attribute_type_idlist
- a11y_incorrect_aria_attribute_type_integer
- a11y_incorrect_aria_attribute_type_token
- a11y_incorrect_aria_attribute_type_tokenlist
- a11y_incorrect_aria_attribute_type_tristate
- a11y_interactive_supports_focus
- a11y_invalid_attribute
- a11y_label_has_associated_control
- a11y_media_has_caption
- a11y_misplaced_role
- a11y_misplaced_scope
- a11y_missing_attribute
- a11y_missing_content
- a11y_mouse_events_have_key_events
- a11y_no_abstract_role
- a11y_no_interactive_element_to_noninteractive_role
- a11y_no_noninteractive_element_interactions
- a11y_no_noninteractive_element_to_interactive_role
- a11y_no_noninteractive_tabindex
- a11y_no_redundant_roles
- a11y_no_static_element_interactions
- a11y_positive_tabindex
- a11y_role_has_required_aria_props
- a11y_role_supports_aria_props
- a11y_role_supports_aria_props_implicit
- a11y_unknown_aria_attribute
- a11y_unknown_role
- attribute_avoid_is
- attribute_global_event_reference
- attribute_illegal_colon
- attribute_invalid_property_name
- attribute_quoted
- bidirectional_control_characters
- bind_invalid_each_rest
- block_empty
- component_name_lowercase
- css_unused_selector
- custom_element_props_identifier
- element_implicitly_closed
- element_invalid_self_closing_tag
- event_directive_deprecated
- export_let_unused
- legacy_code
- legacy_component_creation
- node_invalid_placement_ssr
- non_reactive_update
- options_deprecated_accessors
- options_deprecated_immutable
- options_missing_custom_element
- options_removed_enable_sourcemap
- options_removed_hydratable
- options_removed_loop_guard_timeout
- options_renamed_ssr_dom
- perf_avoid_inline_class
- perf_avoid_nested_class
- reactive_declaration_invalid_placement
- reactive_declaration_module_script_dependency
- script_context_deprecated
- script_unknown_attribute
- slot_element_deprecated
- state_referenced_locally
- store_rune_conflict
- svelte_component_deprecated
- svelte_element_invalid_this
- svelte_self_deprecated
- unknown_code

## Internal-only
- a11y-structure
- invalid-rune-usage
- missing-declaration
- parse-error
<!-- COVERAGE:END -->
