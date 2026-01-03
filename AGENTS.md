# CLAUDE.md

Rust drop-in replacement for `svelte-check` (**Svelte 5+ only**).

**Rust**: Edition 2021, MSRV 1.75

## Commands

```bash
cargo build                    # Build all crates
cargo test                     # Run all tests
cargo test -p <crate>          # Test specific crate (svelte-parser, source-map, etc.)
cargo test --test snapshots    # Run snapshot tests only
cargo test --test corpus_test  # Run corpus/fixture tests only
cargo clippy --all-targets -- -D warnings  # Lint (warnings as errors)
cargo fmt                      # Format (always run before committing)
cargo run -p svelte-check-rs -- --workspace ./path/to/project [--emit-ts]
```

**Snapshots**: Located in `crates/*/tests/snapshots/`. To accept new snapshots:
```bash
# Accept all new snapshots
find crates -name "*.snap.new" -exec sh -c 'mv "$1" "${1%.new}"' _ {} \;
```

## Conventions

**Git**: Use [Conventional Commits](https://www.conventionalcommits.org/).
- Types: `feat`, `fix`, `docs`, `refactor`, `test`, `perf`, `build`, `ci`, `chore`
- Scopes: `parser`, `transformer`, `diagnostics`, `a11y`, `css`, `cli`, `tsgo`
- Example: `feat(parser): add support for snippet blocks`

**Code**:
- Never panic on user input - always return `Result` with errors
- Use `SmolStr` for identifiers, `String` for large content
- All AST nodes must have a `Span`
- Prefer exhaustive pattern matching over `_` wildcards for enums
- Use `insta` snapshot tests for parser/transformer output

## Releasing

Uses [cargo-dist](https://github.com/axodotdev/cargo-dist) for cross-platform binaries.

```bash
# 1. Update version in Cargo.toml (workspace.package.version)
# 2. Commit: git commit -am "chore: release v0.x.x"
# 3. Tag and push:
git tag v0.x.x && git push && git push --tags
# 4. Wait for workflow to complete (~5 min)
gh run watch
```

**Important**: Do NOT manually create GitHub releases — cargo-dist creates the release and uploads binaries automatically. Monitor at: https://github.com/pheuter/svelte-check-rs/actions

## References

- [Svelte 5 Docs](https://svelte.dev/docs)
- [svelte-check source](https://github.com/sveltejs/language-tools/tree/master/packages/svelte-check)
