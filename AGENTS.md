# svelte-check-rs

Rust drop-in replacement for `svelte-check` (**Svelte 5+ only**).

**Rust**: Edition 2021, MSRV 1.75

## Project Structure

- `crates/source-map`: Source position tracking and mapping.
- `crates/svelte-parser`: Svelte 5 syntax parser (lexer, AST, parser).
- `crates/svelte-transformer`: Svelte-to-TypeScript transformation for type-checking.
- `crates/svelte-diagnostics`: Svelte-specific diagnostics (a11y, CSS, component).
- `crates/tsgo-runner`: Bridge to `tsgo` for TypeScript type-checking.
- `crates/bun-runner`: Bridge to bun-managed Svelte compiler diagnostics.
- `crates/svelte-check-rs`: Main CLI and orchestration logic.
- `test-fixtures/`: Svelte component fixtures for testing.
- `node_modules/.cache/svelte-check-rs/`: Per-project cache (transformed files, tsgo incremental build info, bun runner script).

## Architecture

**Pipeline** (orchestrator.rs): File discovery → Parse → Svelte diagnostics → Svelte compiler diagnostics (bun) → Transform to TS → tsgo type-check

1. **Discovery**: Walk workspace, filter by `.svelte`/`.svelte.ts`/`.svelte.js`, respect `tsconfig.json` excludes
2. **Parse**: `svelte_parser::parse()` → AST + parse errors (parallel via `rayon`)
3. **Diagnostics**: A11y, CSS, component checks on AST
4. **Compiler**: Run Svelte compiler diagnostics via bun on original sources
5. **Transform**: `svelte_transformer::transform()` → TypeScript with source maps
6. **Type-check**: Send all transformed files to `tsgo` subprocess, map errors back via source maps

**tsgo Integration** (tsgo-runner crate):
- External TypeScript type-checker (Go-based, faster than tsc)
- Auto-installed to cache dir on first run if not found
- Communication: JSON over stdin/stdout
- Incremental builds via `node_modules/.cache/svelte-check-rs/tsgo.tsbuildinfo`

**Svelte Compiler Integration** (bun-runner crate):
- bun-managed persistent workers that call `svelte/compiler`
- Auto-installed to cache dir on first run if not found
- Diagnostics reported against original `.svelte` sources (no extra source maps)

**SvelteKit Support**:
- Detects route files (`+page.svelte`, `+layout.svelte`, etc.) for proper prop types
- Runs `svelte-kit sync` before type-checking to generate `.svelte-kit/` types
- Resolves `$lib` and other aliases from `svelte.config.js`

## Commands

```bash
cargo build                    # Build all crates
cargo test                     # Run all tests
cargo clippy --all-targets -- -D warnings  # Lint (warnings as errors)
cargo fmt                      # Format (always run before committing)
cargo run -p svelte-check-rs -- --workspace ./path/to/project [--emit-ts]
```

**Useful CLI flags for development**:
- `--emit-ts`: Print transformed TypeScript (debugging transforms)
- `--emit-ast`: Print parsed AST for each file (debugging parser)
- `--emit-source-map`: Print source map mappings (debugging position mapping)
- `--output json`: Machine-readable diagnostics with exact locations
- `--timings`: Show parse/transform/check timing breakdown
- `--cache-stats`: Show cache statistics (files written/skipped)
- `--no-cache`: Disable per-project cache + incremental builds (fresh run)
- `--watch`: File watcher mode (uses `notify` crate, 1s polling)
- `--debug-paths`: Show resolved tsgo, bun, package manager, svelte-kit paths
- `--show-config`: Print resolved configuration (tsconfig, svelte.config.js, excludes)
- `--list-files`: List files that would be checked, then exit
- `--single-file <path>`: Process only a single file (isolate issues)
- `--skip-tsgo`: Skip TypeScript type-checking, only run Svelte diagnostics (faster iteration)
- `--skip-svelte-compiler`: Skip Svelte compiler diagnostics
- `--bun-version`: Show installed bun version/path
- `--bun-update[=<ver>]`: Update bun to latest or specific version

## Testing

**Fixtures** (`test-fixtures/`):

- `valid/` - Components that must parse without errors (includes `a11y/`, `parser/` subdirs)
- `invalid/` - Components with semantic issues (a11y, type errors) - must not panic
- `projects/` - Full SvelteKit projects for integration tests (`sveltekit-bundler/`, etc.)

**Test Types**:

| Test | Command | Purpose |
|------|---------|---------|
| Snapshots | `cargo test --test snapshots` | Parser/transformer output verification |
| Corpus | `cargo test --test corpus_test` | All fixtures parse without panics |
| Integration | `cargo test --test integration_*` | Full CLI workflow with project fixtures |
| Unit | `cargo test -p <crate>` | In-source `mod tests` blocks |

**Snapshots** (uses `insta`): Located in `crates/*/tests/snapshots/`.

```rust
// Parser snapshot - outputs source, errors, AST
parse_snapshot("my_feature", "<div>{value}</div>");

// Transformer snapshot - outputs source, TSX, source map count
transform_snapshot("my_rune", r#"<script>let x = $state(0);</script>"#);
```

```bash
# Accept new snapshots
find crates -name "*.snap.new" -exec sh -c 'mv "$1" "${1%.new}"' _ {} \;
# Or interactively
cargo insta review
```

**Adding Tests**:

- **New fixture**: Add `.svelte` file to `test-fixtures/valid/` or `invalid/` - automatically picked up by corpus tests
- **New snapshot**: Add test fn in `crates/*/tests/snapshots.rs` using `parse_snapshot()` or `transform_snapshot()`
- **New integration test**: Add to `crates/svelte-check-rs/tests/integration_*.rs`, use `#[serial]` attribute

**Testing Expected Errors** (integration tests use JSON output for precise location verification):

```rust
// Define expected diagnostic with exact location
let expected = ExpectedDiagnostic {
    filename: "issue-20-no-pragma/+page.svelte",
    line: 6,
    code: "a11y-no-noninteractive-tabindex",
    message_contains: "tabindex",
};
assert_diagnostic_present(&diagnostics, &expected);

// Or verify no diagnostics in a file
assert_no_diagnostics_in_file(&diagnostics, "valid-file/+page.svelte");
```

**Key Invariants**:

- Valid fixtures must produce zero parse errors
- Invalid fixtures must not panic (errors are expected)
- All new parser features need both snapshot and corpus coverage
- Diagnostics must report correct source line/column (verify via JSON output in integration tests)
- Prefer static fixtures over dynamic generation - integration tests should use real `.svelte` files in `test-fixtures/projects/` to test the full pipeline with actual file I/O

**CI Requirements**: Tests run on Ubuntu/macOS/Windows. Integration tests require `bun` for fixture dependencies.

## Adding A11y Rules

1. Add variant to `DiagnosticCode` enum in `svelte-diagnostics/src/diagnostic.rs`
2. Implement `default_severity()` and `as_str()` for the new code
3. Add check logic in `svelte-diagnostics/src/a11y/mod.rs` within `check_element()`
4. Add test fixture to `test-fixtures/valid/a11y/` or `test-fixtures/invalid/a11y/`
5. Add unit test in `a11y/mod.rs` `#[cfg(test)]` block

Rules support `<!-- svelte-ignore code -->` comments (both `kebab-case` and `snake_case`, wildcards like `a11y-*`).

## Conventions

**Git**: Use [Conventional Commits](https://www.conventionalcommits.org/).

- Types: `feat`, `fix`, `docs`, `refactor`, `test`, `perf`, `build`, `ci`, `chore`
- Scopes: optional; use semantic scopes as needed (examples: `parser`, `transformer`, `diagnostics`, `a11y`, `css`, `cli`, `tsgo`, `bun`, `compiler`). Not a closed list.
- Example: `feat(parser): add support for snippet blocks`

**Code**:

- Never panic on user input - always return `Result` with errors
- Use `camino::Utf8PathBuf` for all filesystem paths (UTF-8 by default)
- Use `smol_str::SmolStr` for identifiers, `String` for large content
- All AST nodes must have a `source_map::Span`
- Prefer exhaustive pattern matching over `_` wildcards for enums
- Error Reporting: Use `miette` for diagnostic reporting and `thiserror` for error types
- Async/Parallel: `tokio` for I/O and process running, `rayon` for CPU-bound tasks

## Releasing

Uses [cargo-dist](https://github.com/axodotdev/cargo-dist).

```bash
# 1. Update version in Cargo.toml (workspace.package.version)
# 2. Commit: git commit -am "chore: release v0.x.x"
# 3. Tag and push:
git tag v0.x.x && git push && git push --tags
# 4. Wait for workflow to complete (~5 min)
gh run watch
```

**Important**: Do NOT manually create GitHub releases. Monitor with: `gh run list` or `gh run watch`
