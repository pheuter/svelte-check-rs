# svelte-check-rs

> **Sunset · September 5, 2026.** This project is no longer maintained and the repository is archived. Use the official [svelte-check](https://github.com/sveltejs/language-tools/tree/master/packages/svelte-check) for new and existing projects. The source, releases, and benchmarks remain available as a historical reference under the MIT license; no further fixes or releases are planned.

## Why this project existed

I started svelte-check-rs in January 2026, before official svelte-check offered
TSGO or incremental compilation. Type-checking a large Svelte application could
interrupt development for long enough to break the flow, especially when running
checks repeatedly in CI or through coding agents.

A Rust parser, parallel Svelte-to-TypeScript transforms, native TypeScript checking,
and persistent caches made that loop much shorter. Early measurements on my
workloads showed order-of-magnitude improvements over the official checker of
the time. Those gains were the reason to build and share a Svelte 5+ replacement;
they are historical results, not a claim about today's svelte-check.

## Why sunset it now?

Official svelte-check now supports native TypeScript checking and incremental
caching. Its [implementation PR](https://github.com/sveltejs/language-tools/pull/2932)
explicitly credits svelte-check-rs and svelte-fast-check as inspirations. Seeing
those improvements reach the official tooling is a meaningful outcome for this
project.

The [September 2026 benchmarks](#benchmarks) still show a Rust performance
advantage, but the practical gap has narrowed considerably. On a synthetic
500-component project, warm checks took 0.35 seconds upstream and 0.16 seconds in
Rust. On Careswitch Web, the timings were 3.14 and 2.20 seconds, with materially
different diagnostics that prevent an equivalent-checking speedup claim.

Maintaining a separate parser, transformer, source mappings, and integrations
requires ongoing work to follow Svelte, SvelteKit, and TypeScript. That cost is no
longer justified by the remaining speed advantage. The recommendation is now to
use the official checker and focus compatibility and performance improvements
there.

## Moving to official svelte-check

Use [svelte-check in sveltejs/language-tools](https://github.com/sveltejs/language-tools/tree/master/packages/svelte-check)
and follow its current installation and CLI documentation.

- Replace the `svelte-check-rs` development dependency with `svelte-check` using your package manager.
- Replace `svelte-check-rs` in package scripts, CI jobs, and agent instructions with `svelte-check`. Keep any existing `svelte-kit sync` step.
- Review flags against upstream's documentation; Rust-specific options are not portable. Re-run checks and review diagnostics before relying on the migrated command.
- For native checking, follow upstream's TypeScript dependency instructions. `--tsgo --incremental` enables both native checking and caching, but has documented limitations, including Svelte files outside the tsconfig root. The newer `--tsgo-experimental-api` mode has different tradeoffs and is experimental. Choose the mode appropriate for your project rather than assuming diagnostic parity.

This sunset is not a claim that every project will produce identical diagnostics
after switching. The [benchmark notes](benchmarks/README.md#careswitch-web) record
known differences from the real application we measured.

## Thank you

Thank you to everyone who tried svelte-check-rs, contributed code, opened issues,
shared reproductions and benchmarks, tested releases, or helped someone use it.
Every contribution helped this project handle more of the Svelte ecosystem and
made it useful beyond my own application.

Thank you also to the Svelte and language-tools maintainers, the TypeScript native
compiler team, and the wider community whose work made this possible. Please
bring future Svelte checking improvements and reproducible issues to the
[official project](https://github.com/sveltejs/language-tools), following its
contribution guidance.

— [Mark Fayngersh (@pheuter)](https://github.com/pheuter)

## Benchmarks

<!-- BENCHMARKS:START -->
Median of 5 runs, measured 2026-09-05. svelte-check 4.7.6 vs svelte-check-rs 0.11.2.

Apple M4 Max · 48 GiB · macOS 27.0 · Node 24.20.0 · Bun 1.4.0.

| Workload | Scenario | svelte-check (TS6) | svelte-check (TS7 + incremental) | svelte-check-rs | Speedup vs TS7 |
| --- | --- | --- | --- | --- | --- |
| 500 components (synthetic) | Cold cache | 1.284 s | 0.896 s | 0.413 s | 2.2× |
| 500 components (synthetic) | Warm cache | 1.299 s | 0.350 s | 0.156 s | 2.3× |
| 500 components (synthetic) | Repeated TS edits | 1.347 s | 0.374 s | 0.168 s | 2.2× |

All timed public-fixture runs reported **0 errors and 0 warnings**.

Careswitch Web (private monorepo) has **different diagnostic results**: upstream TS7: **436 errors / 3 warnings**; Rust: **2 errors / 551 warnings**. These timings do not establish equivalent checking; no speedup claim is made for this workload.

| Workload | Scenario | svelte-check (TS7 + incremental) | svelte-check-rs |
| --- | --- | --- | --- |
| Careswitch Web | Cold cache | 16.789 s | 11.460 s |
| Careswitch Web | Warm cache | 3.140 s | 2.204 s |
| Careswitch Web | Repeated TS edits | 3.490 s | 2.408 s |
<!-- BENCHMARKS:END -->

The TS7 baseline is `svelte-check --tsgo --incremental`, with both tools using
TypeScript 7.0.2. Cold runs clear checker caches; warm runs preserve them;
repeated-edit runs change one typed TypeScript export's value after an untimed
priming edit. These are
separate CLI processes, not watch-mode updates. Results depend on the workload.

See [methodology, reproduction commands, and raw samples](benchmarks/README.md)
for versions, diagnostic checks, and limitations.

## Historical reference

The following documents the final release for existing users and anyone studying
or forking the code. It is not a recommendation to install this unmaintained tool.

<details>
<summary>Archived installation, usage, and development documentation</summary>

## Installation

### npm

```bash
npm install -D svelte-check-rs
```

The npm package uses platform-specific optional dependencies to provide the binary. If you install with `--no-optional`, re-enable optional dependencies or use the shell/PowerShell installers below.

Then add to your package.json scripts:

```json
{
  "scripts": {
    "check": "svelte-check-rs"
  }
}
```

Or run directly with npx:

```bash
npx svelte-check-rs
```

### macOS / Linux

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/pheuter/svelte-check-rs/releases/latest/download/svelte-check-rs-installer.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://github.com/pheuter/svelte-check-rs/releases/latest/download/svelte-check-rs-installer.ps1 | iex
```

## Usage

```bash
# Check current directory
svelte-check-rs

# Check specific directory
svelte-check-rs --workspace ./my-project

# Watch mode
svelte-check-rs --watch

# Different output formats
svelte-check-rs --output json
svelte-check-rs --output machine
svelte-check-rs --output human-verbose
```

## Requirements

`svelte-check-rs` supports the released TypeScript 7 native compiler. For Svelte
and SvelteKit projects, keep TypeScript 6 installed for tooling that uses its
JavaScript API and install the native compiler alongside it:

```bash
npm install -D typescript@^6 @typescript/native@npm:typescript@^7.0.2
```

The checker resolves `@typescript/native` directly, so a shared `tsc` shim cannot
accidentally select TypeScript 6. A regular `typescript` installation at version
7 or later is also supported when your other tooling no longer needs the
TypeScript 6 API. Replacing TypeScript 6 outright can break SvelteKit type
generation; see the [upstream compatibility discussion](https://github.com/sveltejs/language-tools/issues/3063#issuecomment-5472405798).

Existing `@typescript/native-preview` installations remain supported, starting at
`7.0.0-dev.20260707.2` for consistent UTF-16 diagnostic columns. Install one of the
native compiler options explicitly; both npm peers are optional to allow either
choice. Compiler resolution walks up from `--workspace` through ancestor
`node_modules` directories.

Configured preprocessors are resolved with Vite-first precedence: effective options from
`vite.config.*` are used when vite-plugin-svelte or SvelteKit exposes them; otherwise
`svelte.config.{js,cjs,mjs,ts,mts}` is loaded. Inline Vite preprocessors and the
vite-plugin-svelte `configFile` option are supported. Preprocessor and imported config
dependencies are monitored in watch mode, including files outside the workspace.

### CLI Options

| Option | Description |
|--------|-------------|
| `--workspace <PATH>` | Working directory (default: `.`) |
| `--output <FORMAT>` | Output format: `human`, `human-verbose`, `json`, `machine` |
| `--color <MODE>` | Human output colors: `auto` (terminal only), `always`, `never`. Nonempty `NO_COLOR` disables colors. JSON and machine output stay plain. |
| `--tsconfig <PATH>` | Path to tsconfig.json |
| `--threshold <LEVEL>` | Minimum severity: `error`, `warning` |
| `--watch` | Watch mode |
| `--preserveWatchOutput` | Don't clear screen in watch mode |
| `--fail-on-warnings` | Exit with error on warnings |
| `--ignore <PATTERNS>` | Glob patterns to ignore |
| `--skip-tsgo` | Skip TypeScript type-checking |
| `--tsgo-version` | Show installed tsgo version + path |
| `--bun-version` | Show installed bun version + path |
| `--bun-update[=<VER>]` | Update bun to latest or specific version |
| `--debug-paths` | Show resolved binaries (tsgo, bun, svelte-kit) |

**Caching:** svelte-check-rs writes transformed files and tsgo incremental build info to `node_modules/.cache/svelte-check-rs/`. Cache invalidation is automatic: dependency changes (lockfiles, node_modules markers) clear the entire cache, and source file changes are handled via content-addressed writes.

## Project Structure

```
crates/
├── svelte-parser/        # Lexer + parser + AST types
├── source-map/           # Position tracking and mapping
├── svelte-transformer/   # Svelte → TypeScript transformation
├── svelte-diagnostics/   # A11y and component checks
├── tsgo-runner/          # tsgo process management
├── bun-runner/           # bun-managed Svelte compiler bridge
└── svelte-check-rs/      # CLI binary
```

## Development

```bash
# Build all crates
cargo build

# Run tests
cargo test

# Run clippy
cargo clippy --all-targets -- -D warnings

# Format code
cargo fmt
```

### Upstream parser parity sweep

To compare this parser against Svelte's full parser suites (`parser-modern` + `parser-legacy`),
run the optional ignored test with a local checkout of `sveltejs/svelte`:

```bash
git clone https://github.com/sveltejs/svelte.git /tmp/svelte
SVELTE_REPO=/tmp/svelte cargo test -p svelte-parser test_upstream_svelte_parser_samples -- --ignored
```

The harness runs every sample under `parser-modern` and `parser-legacy`, enabling loose mode
for samples whose directory name starts with `loose-` (mirroring upstream's runner).

</details>

## License

MIT License - see [LICENSE](LICENSE) for details.
