# svelte-check-rs

A high-performance, Rust-powered diagnostic engine designed as a drop-in replacement for `svelte-check`.

> **Note:** This tool only supports **Svelte 5+**. For Svelte 4 or earlier, use the official [svelte-check](https://github.com/sveltejs/language-tools/tree/master/packages/svelte-check).

## Features

- 🚀 **Fast**: 10-100x faster than `svelte-check` through Rust's zero-cost abstractions and parallel processing
- ✅ **Accurate**: Matches `svelte-check` diagnostics, including Svelte compiler errors via bun
- 🔄 **Compatible**: Drop-in CLI replacement, identical output formats
- 🔧 **Maintainable**: Clean separation of concerns, comprehensive test suite

## Installation

### npm (recommended)

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

### CLI Options

| Option | Description |
|--------|-------------|
| `--workspace <PATH>` | Working directory (default: `.`) |
| `--output <FORMAT>` | Output format: `human`, `human-verbose`, `json`, `machine` |
| `--tsconfig <PATH>` | Path to tsconfig.json |
| `--threshold <LEVEL>` | Minimum severity: `error`, `warning` |
| `--watch` | Watch mode |
| `--preserveWatchOutput` | Don't clear screen in watch mode |
| `--fail-on-warnings` | Exit with error on warnings |
| `--ignore <PATTERNS>` | Glob patterns to ignore |
| `--no-cache` | Disable per-project cache + incremental builds (fresh run) |
| `--disable-sveltekit-cache` | Disable cached .svelte-kit mirror |
| `--skip-tsgo` | Skip TypeScript type-checking |
| `--tsgo-version` | Show installed tsgo version + path |
| `--tsgo-update[=<VER>]` | Update tsgo to latest or specific version |
| `--bun-version` | Show installed bun version + path |
| `--bun-update[=<VER>]` | Update bun to latest or specific version |
| `--debug-paths` | Show resolved binaries (tsgo, bun, svelte-kit) |

**Caching:** By default, svelte-check-rs writes transformed files and tsgo incremental build info to `node_modules/.cache/svelte-check-rs/`. Use `--no-cache` for a fully fresh run (useful in CI).

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

## License

MIT License - see [LICENSE](LICENSE) for details.
