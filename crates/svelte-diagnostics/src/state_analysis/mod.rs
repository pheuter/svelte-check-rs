//! State reference analysis for Svelte 5 runes.
//!
//! This module detects `state_referenced_locally` warnings, which occur when
//! reactive state is referenced in a way that won't update reactively.
//!
//! The warning triggers when:
//! 1. A variable is bound to a reactive source (`$state`, `$props`, `$derived`, `$state.raw`)
//! 2. That variable is read (not assigned/updated)
//! 3. The read happens at the same function depth as the binding (not inside a closure)

mod scope;
mod visitor;

use crate::{Diagnostic, DiagnosticCode};
use source_map::Span;
use swc_common::{sync::Lrc, FileName, SourceMap};
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

pub use scope::{Binding, BindingKind, Scope};
pub use visitor::StateAnalyzer;

/// Analyzes script content for state_referenced_locally warnings.
pub fn analyze_script(content: &str, content_span: Span, is_typescript: bool) -> Vec<Diagnostic> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(Lrc::new(FileName::Anon), content.to_string());

    let syntax = if is_typescript {
        Syntax::Typescript(TsSyntax {
            tsx: false,
            decorators: true,
            dts: false,
            no_early_errors: true,
            disallow_ambiguous_jsx_like: false,
        })
    } else {
        Syntax::Es(Default::default())
    };

    let lexer = Lexer::new(syntax, Default::default(), StringInput::from(&*fm), None);
    let mut parser = Parser::new_from(lexer);

    let module = match parser.parse_module() {
        Ok(m) => m,
        Err(_) => return Vec::new(), // Parse errors are handled elsewhere
    };

    let mut analyzer = StateAnalyzer::new(content_span);
    analyzer.analyze(&module);
    analyzer.into_diagnostics()
}

/// Creates a state_referenced_locally diagnostic.
pub(crate) fn create_diagnostic(name: &str, span: Span, suggestion_type: &str) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::StateReferencedLocally,
        format!(
            "This reference only captures the initial value of `{}`. Did you mean to reference it inside a {} instead?\nhttps://svelte.dev/e/state_referenced_locally",
            name, suggestion_type
        ),
        span,
    )
}
