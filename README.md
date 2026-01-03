# svelte-check-rs

A high-performance, Rust-powered diagnostic engine designed as a drop-in replacement for `svelte-check`.

> **Note:** This tool only supports **Svelte 5+**. For Svelte 4 or earlier, use the official [svelte-check](https://github.com/sveltejs/language-tools/tree/master/packages/svelte-check).

## Features

- 🚀 **Fast**: 10-100x faster than `svelte-check` through Rust's zero-cost abstractions and parallel processing
- ✅ **Accurate**: Full feature parity with `svelte-check` - same diagnostics, same behavior
- 🔄 **Compatible**: Drop-in CLI replacement, identical output formats
- 🔧 **Maintainable**: Clean separation of concerns, comprehensive test suite

## Installation

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
| `--diagnostic-sources <LIST>` | Which diagnostics: `js`, `svelte`, `css` |
| `--ignore <PATTERNS>` | Glob patterns to ignore |

## Project Structure

```
crates/
├── svelte-parser/        # Lexer + parser + AST types
├── source-map/           # Position tracking and mapping
├── svelte-transformer/   # Svelte → TypeScript transformation
├── svelte-diagnostics/   # A11y, CSS, and component checks
├── tsgo-runner/          # tsgo process management
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
