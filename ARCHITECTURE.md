# Svelte-Check-RS: Architecture & Implementation Plan

## Executive Summary

**Svelte-Check-RS** is a high-performance, Rust-powered diagnostic engine designed as a drop-in replacement for `svelte-check`. It transforms Svelte components into TypeScript/TSX for type-checking via the Go-based TypeScript compiler (`tsgo`), while also performing Svelte-specific diagnostics (accessibility, unused CSS, component validation).

> **Important:** This tool exclusively supports **Svelte 5+**. It leverages Svelte 5's runes (`$props`, `$state`, `$derived`, etc.) and does not provide backwards compatibility with Svelte 4's `export let` syntax or legacy reactive statements (`$:`). For projects using Svelte 4 or earlier, use the official [svelte-check](https://github.com/sveltejs/language-tools/tree/master/packages/svelte-check).

### Key Goals
- **Performance**: 10-100x faster than `svelte-check` through Rust's zero-cost abstractions and parallel processing
- **Accuracy**: Full feature parity with `svelte-check` - same diagnostics, same behavior
- **Compatibility**: Drop-in CLI replacement, identical output formats
- **Maintainability**: Clean separation of concerns, comprehensive test suite

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              CLI Layer                                       │
│                         (svelte-check-rs crate)                             │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────┐  ┌──────────────────┐  │
│  │ Arg Parser  │  │ Config Loader│  │ Output      │  │ Watch Mode       │  │
│  │ (clap)      │  │ (svelte.cfg) │  │ Formatter   │  │ (notify)         │  │
│  └─────────────┘  └──────────────┘  └─────────────┘  └──────────────────┘  │
└─────────────────────────────────────┬───────────────────────────────────────┘
                                      │
┌─────────────────────────────────────▼───────────────────────────────────────┐
│                          Orchestration Layer                                 │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                    Diagnostic Collector                               │   │
│  │  - Aggregates diagnostics from all sources                           │   │
│  │  - Deduplicates and sorts by file/line                               │   │
│  │  - Maps generated positions back to source                           │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│  ┌─────────────────────────────┐  ┌─────────────────────────────────────┐   │
│  │   Project Discovery         │  │   Incremental Cache                 │   │
│  │   - Find svelte.config.js   │  │   - Track file hashes              │   │
│  │   - Resolve tsconfig.json   │  │   - Cache transformed output       │   │
│  │   - Glob .svelte files      │  │   - Invalidation logic             │   │
│  └─────────────────────────────┘  └─────────────────────────────────────┘   │
└─────────────────────────────────────┬───────────────────────────────────────┘
                                      │
          ┌───────────────────────────┼───────────────────────────┐
          │                           │                           │
          ▼                           ▼                           ▼
┌─────────────────────┐  ┌─────────────────────────┐  ┌─────────────────────┐
│  Svelte Diagnostics │  │  Svelte → TSX Transform │  │   tsgo Runner       │
│  (svelte-diagnostics│  │  (svelte-transformer)   │  │   (tsgo-runner)     │
│   crate)            │  │                         │  │                     │
│                     │  │  Converts .svelte to    │  │  - Spawns tsgo      │
│  - A11y checks      │  │  type-checkable .tsx    │  │  - Parses output    │
│  - Unused CSS       │  │                         │  │  - Maps diagnostics │
│  - Unused exports   │  │                         │  │                     │
│  - Component rules  │  │                         │  │                     │
└─────────┬───────────┘  └───────────┬─────────────┘  └──────────┬──────────┘
          │                          │                           │
          │                          ▼                           │
          │              ┌─────────────────────────┐              │
          │              │     Source Mapper       │              │
          │              │     (source-map crate)  │◄─────────────┘
          │              │                         │
          │              │  - Position tracking    │
          │              │  - Span mapping         │
          │              │  - Line/col conversion  │
          │              └───────────┬─────────────┘
          │                          │
          ▼                          ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                            Svelte Parser                                     │
│                         (svelte-parser crate)                               │
│                                                                              │
│  ┌─────────────┐  ┌─────────────────┐  ┌────────────────────────────────┐   │
│  │   Lexer     │  │   Parser        │  │   AST Types                    │   │
│  │   (logos)   │  │   (hand-written │  │                                │   │
│  │             │  │    recursive    │  │   - SvelteDocument             │   │
│  │   Tokens:   │  │    descent)     │  │   - Script (module/instance)   │   │
│  │   - HTML    │  │                 │  │   - Style                      │   │
│  │   - Svelte  │  │   Produces:     │  │   - Fragment (template)        │   │
│  │   - JS/TS   │  │   - CST/AST     │  │   - Element/Component          │   │
│  │             │  │   - Errors      │  │   - Expression                 │   │
│  │             │  │   - Trivia      │  │   - Block (if/each/await/etc)  │   │
│  └─────────────┘  └─────────────────┘  └────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Crate Breakdown

### 1. `svelte-parser`
**Purpose**: Parse `.svelte` files into a structured AST

**Key Responsibilities**:
- Tokenize Svelte syntax (HTML + Svelte blocks + embedded JS/TS)
- Parse into a lossless CST (Concrete Syntax Tree) using `rowan`
- Provide error recovery for partial/invalid input
- Track precise source positions for all nodes

**Public API**:
```rust
pub fn parse(source: &str) -> ParseResult;
pub fn parse_with_options(source: &str, options: ParseOptions) -> ParseResult;

pub struct ParseResult {
    pub document: SvelteDocument,
    pub errors: Vec<ParseError>,
}
```

**AST Node Types**:
```rust
pub struct SvelteDocument {
    pub module_script: Option<Script>,    // <script context="module">
    pub instance_script: Option<Script>,  // <script>
    pub style: Option<Style>,             // <style>
    pub fragment: Fragment,               // Template content
}

pub struct Script {
    pub span: Span,
    pub content: String,
    pub content_span: Span,  // Just the JS/TS content
    pub lang: ScriptLang,    // js | ts
    pub context: ScriptContext, // module | default
}

pub enum TemplateNode {
    Element(Element),
    Component(Component),
    Text(Text),
    Expression(Expression),      // {foo}
    HtmlTag(HtmlTag),           // {@html ...}
    ConstTag(ConstTag),         // {@const ...}
    DebugTag(DebugTag),         // {@debug ...}
    RenderTag(RenderTag),       // {@render ...}
    IfBlock(IfBlock),
    EachBlock(EachBlock),
    AwaitBlock(AwaitBlock),
    KeyBlock(KeyBlock),
    SnippetBlock(SnippetBlock), // {#snippet name()}...{/snippet}
    Comment(Comment),
}
```

### 2. `source-map`
**Purpose**: Track positions through transformations

**Key Responsibilities**:
- Represent source locations (byte offsets, line/column)
- Build mappings during transformation
- Query original position from generated position
- Efficient line index computation

**Public API**:
```rust
pub struct SourceMap {
    // Maps generated offset → original offset
    mappings: Vec<Mapping>,
}

impl SourceMap {
    pub fn builder() -> SourceMapBuilder;
    pub fn original_position(&self, generated: ByteOffset) -> Option<ByteOffset>;
    pub fn generated_position(&self, original: ByteOffset) -> Option<ByteOffset>;
}

pub struct SourceMapBuilder {
    pub fn add_mapping(&mut self, original: Span, generated: Span);
    pub fn add_source(&mut self, original: ByteOffset, len: u32);
    pub fn add_generated(&mut self, text: &str);
    pub fn build(self) -> SourceMap;
}

pub struct LineIndex {
    pub fn new(text: &str) -> Self;
    pub fn line_col(&self, offset: ByteOffset) -> LineCol;
    pub fn offset(&self, line_col: LineCol) -> ByteOffset;
}
```

### 3. `svelte-transformer`
**Purpose**: Convert Svelte AST to type-checkable TypeScript/TSX

**Key Responsibilities**:
- Transform template to TSX for type-checking expressions
- Generate type definitions for props (`$props()`)
- Handle runes: `$state`, `$derived`, `$effect`, `$bindable`
- Transform slots, events, bindings to typed equivalents
- Produce source maps for position mapping

**Transformation Strategy**:

```svelte
<!-- Input: Counter.svelte -->
<script lang="ts">
  interface Props {
    initial?: number;
    onchange?: (value: number) => void;
  }
  
  let { initial = 0, onchange }: Props = $props();
  let count = $state(initial);
  let doubled = $derived(count * 2);
  
  function increment() {
    count++;
    onchange?.(count);
  }
</script>

<button onclick={increment}>
  Count: {count} (doubled: {doubled})
</button>
```

```tsx
// Output: Counter.svelte.tsx (for type-checking)
import { SvelteComponent } from 'svelte';

// === INSTANCE SCRIPT ===
interface Props {
  initial?: number;
  onchange?: (value: number) => void;
}

let { initial = 0, onchange }: Props = $props();
let count: number = initial;           // $state unwrapped
let doubled: number = (count * 2);     // $derived unwrapped

function increment() {
  count++;
  onchange?.(count);
}

// === TEMPLATE TYPE-CHECK BLOCK ===
// This is never executed, just type-checked
function __svelte_template_check__() {
  // Element expressions
  increment;  // onclick handler
  count;      // expression {count}
  doubled;    // expression {doubled}
}

// === COMPONENT TYPE EXPORT ===
export default class Counter extends SvelteComponent<Props, {}, {}> {}
```

**Rune Transformations**:
| Rune | Transformation |
|------|----------------|
| `$props()` | Extract type, generate component generics |
| `$state(init)` | `let x: InferredType = init;` |
| `$state.raw(init)` | Same as `$state` |
| `$derived(expr)` | `let x: InferredType = (expr);` |
| `$derived.by(fn)` | `let x: ReturnType<typeof fn> = fn();` |
| `$effect(() => {})` | `(() => {})();` (for type-checking body) |
| `$effect.pre(() => {})` | Same as `$effect` |
| `$bindable()` | Mark prop as bindable in type |
| `$inspect()` | No-op |
| `$host()` | `this` |

**Public API**:
```rust
pub fn transform(document: &SvelteDocument, options: TransformOptions) -> TransformResult;

pub struct TransformResult {
    pub tsx_code: String,
    pub source_map: SourceMap,
    pub exports: ComponentExports,
}

pub struct ComponentExports {
    pub props_type: Option<String>,
    pub events_type: Option<String>,
    pub slots_type: Option<String>,
}
```

### 4. `svelte-diagnostics`
**Purpose**: Svelte-specific linting and validation

**Diagnostic Categories**:

#### A11y Checks (match svelte-check exactly)
- `a11y-accesskey`: No accesskey attribute
- `a11y-aria-activedescendant-has-tabindex`: Elements with aria-activedescendant must be tabbable
- `a11y-aria-attributes`: Valid aria-* attributes
- `a11y-autofocus`: No autofocus
- `a11y-click-events-have-key-events`: Click handlers need keyboard equivalents
- `a11y-distracting-elements`: No `<marquee>` or `<blink>`
- `a11y-hidden`: No aria-hidden on focusable elements
- `a11y-img-redundant-alt`: No "image" or "picture" in alt text
- `a11y-incorrect-aria-attribute-type`: Correct ARIA attribute value types
- `a11y-interactive-supports-focus`: Interactive elements must be focusable
- `a11y-invalid-attribute`: Valid attribute values
- `a11y-label-has-associated-control`: Labels must have form controls
- `a11y-media-has-caption`: Video elements need captions
- `a11y-missing-attribute`: Required attributes (alt, aria-label, etc.)
- `a11y-missing-content`: Anchors and headings need content
- `a11y-mouse-events-have-key-events`: Mouse events need keyboard events
- `a11y-no-noninteractive-element-interactions`: No handlers on non-interactive elements
- `a11y-no-noninteractive-element-to-interactive-role`: Don't override semantics
- `a11y-no-noninteractive-tabindex`: No tabindex on non-interactive elements
- `a11y-no-redundant-roles`: No redundant ARIA roles
- `a11y-no-static-element-interactions`: No handlers on static elements
- `a11y-positive-tabindex`: No positive tabindex
- `a11y-role-has-required-aria-props`: Roles have required ARIA props
- `a11y-role-supports-aria-props`: ARIA props match role
- `a11y-structure`: Correct heading structure
- (and more...)

#### CSS Checks
- `css-unused-selector`: Selectors that don't match any elements
- `css-invalid-global`: Invalid `:global()` usage

#### Component Checks
- `unused-export-let`: Exported props never used (Svelte 4 compat)
- `missing-declaration`: `$props()` destructures unknown property
- `invalid-rune-usage`: Runes used incorrectly
- `component-name-lowercase`: Component names should be PascalCase

**Public API**:
```rust
pub fn check(document: &SvelteDocument, options: DiagnosticOptions) -> Vec<Diagnostic>;

pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub suggestions: Vec<Suggestion>,
}

pub enum Severity {
    Error,
    Warning,
    Hint,
}
```

### 5. `tsgo-runner`
**Purpose**: Interface with the Go TypeScript compiler

**Key Responsibilities**:
- Spawn `tsgo` process with correct arguments
- Write virtual file system (transformed .tsx files)
- Parse `tsgo` diagnostic output
- Map diagnostic positions back to original .svelte files

**Integration Strategy**:
```rust
pub struct TsgoRunner {
    tsgo_path: PathBuf,
    project_root: PathBuf,
}

impl TsgoRunner {
    pub fn new(tsgo_path: PathBuf, project_root: PathBuf) -> Self;
    
    pub async fn check(&self, files: &TransformedFiles) -> Result<Vec<TsgoDiagnostic>>;
}

pub struct TransformedFiles {
    // Map of virtual path → transformed content + source map
    pub files: HashMap<PathBuf, TransformedFile>,
}

pub struct TransformedFile {
    pub original_path: PathBuf,
    pub tsx_content: String,
    pub source_map: SourceMap,
}
```

**tsgo Invocation**:
```bash
# Write transformed files to temp directory
# Invoke tsgo with project config
tsgo --project /path/to/tsconfig.json --diagnostics
```

### 6. `svelte-check-rs`
**Purpose**: CLI application and orchestration

**CLI Interface** (matches `svelte-check`):
```bash
svelte-check-rs [OPTIONS]

OPTIONS:
    --workspace <PATH>         Working directory
    --output <FORMAT>          Output format: human | human-verbose | json | machine
    --tsconfig <PATH>          Path to tsconfig.json
    --threshold <LEVEL>        Minimum severity: error | warning  
    --watch                    Watch mode
    --preserveWatchOutput      Don't clear screen in watch mode
    --fail-on-warnings         Exit 1 on warnings
    --compiler-warnings <JSON> Configure compiler warning levels
    --diagnostic-sources <LIST> Which diagnostics: js | svelte | css (comma-separated)
    --ignore <PATTERNS>        Glob patterns to ignore
```

**Output Formats**:

Human (default):
```
src/lib/Counter.svelte:15:3
Error: Type 'string' is not assignable to type 'number' (ts)

src/lib/Button.svelte:8:1
Warning: A11y: <img> element should have an alt attribute (a11y-missing-attribute)

====================================
svelte-check found 1 error and 1 warning in 2 files
```

JSON:
```json
[
  {
    "type": "Error",
    "filename": "src/lib/Counter.svelte",
    "start": { "line": 15, "column": 3, "offset": 342 },
    "end": { "line": 15, "column": 18, "offset": 357 },
    "message": "Type 'string' is not assignable to type 'number'",
    "code": "ts(2322)",
    "source": "ts"
  }
]
```

Machine:
```
ERROR src/lib/Counter.svelte:15:3:15:18 Type 'string' is not assignable to type 'number' (ts)
```

---

## Implementation Phases

### Phase 1: Foundation (Week 1-2)
**Goal**: Parse Svelte files and produce basic AST

- [ ] Set up workspace structure and CI
- [ ] Implement Svelte lexer (logos)
- [ ] Implement Svelte parser (recursive descent)
- [ ] Define complete AST types
- [ ] Implement source-map primitives
- [ ] Unit tests with snapshot testing (insta)
- [ ] Test against Svelte component corpus

**Deliverable**: Can parse any valid Svelte 5 file into AST

### Phase 2: Transformation (Week 3-4)
**Goal**: Transform Svelte to type-checkable TSX

- [ ] Implement basic script extraction
- [ ] Implement rune transformations
- [ ] Implement template → TSX conversion
- [ ] Handle props, slots, events typing
- [ ] Generate source maps during transformation
- [ ] Test transformation accuracy

**Deliverable**: Can transform Svelte to TSX with accurate source maps

### Phase 3: tsgo Integration (Week 5)
**Goal**: Run TypeScript checking via tsgo

- [ ] Implement tsgo process spawning
- [ ] Handle virtual file system setup
- [ ] Parse tsgo diagnostic output
- [ ] Map diagnostics back to source positions
- [ ] Handle tsconfig.json resolution

**Deliverable**: End-to-end type checking working

### Phase 4: Svelte Diagnostics (Week 6-7)
**Goal**: Implement Svelte-specific checks

- [ ] Implement all a11y checks
- [ ] Implement CSS unused selector detection
- [ ] Implement component-specific checks
- [ ] Match svelte-check diagnostic messages exactly

**Deliverable**: Feature parity with svelte-check diagnostics

### Phase 5: CLI & Polish (Week 8)
**Goal**: Production-ready CLI

- [ ] Implement CLI argument parsing
- [ ] Implement all output formats
- [ ] Implement watch mode
- [ ] Add configuration file support
- [ ] Performance optimization
- [ ] Comprehensive documentation

**Deliverable**: Drop-in replacement ready for testing

### Phase 6: Validation & Release (Week 9-10)
**Goal**: Validate against real projects

- [ ] Test against SvelteKit projects
- [ ] Test against component libraries
- [ ] Fix edge cases
- [ ] Performance benchmarking
- [ ] Release v0.1.0

---

## Testing Strategy

### Unit Tests
- Parser: Snapshot tests for AST output
- Transformer: Snapshot tests for generated TSX
- Diagnostics: Test each rule in isolation
- Source maps: Position mapping accuracy

### Integration Tests
- End-to-end: Full file → diagnostics pipeline
- Comparison tests: Same output as `svelte-check`

### Corpus Tests
- Parse/transform every file in:
  - SvelteKit repo (`packages/kit/src`)
  - svelte.dev repo
  - Component libraries (skeleton, shadcn-svelte)

### Property Tests
- Round-trip: `parse(source).to_string() ≈ source`
- Position invariants: All spans within bounds

---

## Key Technical Decisions

### Parser: Hand-written vs Parser Generator
**Decision**: Hand-written recursive descent
**Rationale**: 
- Better error recovery
- Easier to maintain
- Better performance
- Parser generators struggle with Svelte's context-dependent syntax

### CST vs AST
**Decision**: Lossless CST with AST view (using `rowan`)
**Rationale**:
- Preserves all source information (trivia, comments)
- Enables accurate source maps
- Supports future formatter/LSP needs

### JS/TS Parsing in Script Blocks
**Decision**: Don't parse JS/TS ourselves - treat as opaque string
**Rationale**:
- `tsgo` will parse and type-check it
- We only need to identify runes and extract type info
- Rune detection can be done with simple pattern matching

### Parallel Processing
**Decision**: Use `rayon` for file-level parallelism
**Rationale**:
- Files are independent until final aggregation
- Easy to implement with `rayon`'s parallel iterators

---

## File Structure

```
svelte-check-rs/
├── Cargo.toml                 # Workspace manifest
├── CLAUDE.md                  # AI assistant context
├── ARCHITECTURE.md            # This document
├── README.md                  # User documentation
├── LICENSE
├── .github/
│   └── workflows/
│       └── ci.yml             # CI pipeline
├── crates/
│   ├── svelte-parser/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── lexer.rs       # Token definitions (logos)
│   │       ├── parser.rs      # Recursive descent parser
│   │       ├── ast.rs         # AST node types
│   │       ├── error.rs       # Parse errors
│   │       └── tests/
│   │           ├── mod.rs
│   │           ├── lexer_tests.rs
│   │           ├── parser_tests.rs
│   │           └── snapshots/
│   ├── source-map/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── span.rs
│   │       ├── line_index.rs
│   │       └── builder.rs
│   ├── svelte-transformer/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── transform.rs   # Main transformation logic
│   │       ├── runes.rs       # Rune handling
│   │       ├── template.rs    # Template → TSX
│   │       ├── types.rs       # Type generation
│   │       └── tests/
│   ├── svelte-diagnostics/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── a11y/          # Accessibility checks
│   │       │   ├── mod.rs
│   │       │   └── rules/
│   │       ├── css/           # CSS checks
│   │       ├── component/     # Component checks
│   │       └── diagnostic.rs  # Diagnostic types
│   ├── tsgo-runner/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── runner.rs
│   │       └── parser.rs      # Parse tsgo output
│   └── svelte-check-rs/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── cli.rs         # Argument parsing
│           ├── config.rs      # Configuration
│           ├── output.rs      # Output formatting
│           ├── watch.rs       # Watch mode
│           └── orchestrator.rs
└── test-fixtures/
    ├── valid/                 # Valid Svelte files
    ├── invalid/               # Files with expected errors
    └── projects/              # Full project structures
```

---

## Dependencies Summary

| Crate | Purpose |
|-------|---------|
| `logos` | Fast lexer generation |
| `rowan` | Lossless syntax trees |
| `text-size` | Text offset types |
| `clap` | CLI argument parsing |
| `serde` / `serde_json` | JSON output |
| `tokio` | Async runtime for tsgo |
| `miette` | Beautiful error display |
| `walkdir` / `globset` | File discovery |
| `notify` | File watching |
| `insta` | Snapshot testing |
| `rayon` | Parallel processing |
| `rustc-hash` | Fast hashing |

---

## Success Criteria

1. **Correctness**: 100% of `svelte-check` diagnostics reproduced
2. **Performance**: 10x faster than `svelte-check` on typical projects
3. **Compatibility**: All CLI flags work identically
4. **Reliability**: No crashes on malformed input
5. **Test Coverage**: >90% code coverage
