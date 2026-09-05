# CLI benchmarks

These final measurements are preserved as a historical record of the project's
[September 2026 sunset](../README.md#why-sunset-it-now). Use official svelte-check
for ongoing development; the benchmark harness is no longer maintained.

Measured on September 5, 2026 with a release build of svelte-check-rs 0.11.2
(`0b6e0d5`), svelte-check 4.7.6 (npm's latest release on that date),
TypeScript 6.0.3, and TypeScript 7.0.2 installed as `@typescript/native`.
Machine: Apple M4 Max, 16 logical CPUs, 48 GiB RAM, macOS 27.0;
Node 24.20.0, Bun 1.4.0. Exact metadata and individual wall-clock samples are
stored in [results.json](results.json).

## Public workloads

The generator creates 100 or 500 Svelte components, with the same number of
TypeScript modules. Components use typed props, `$state`, `$derived`, snippets,
event handlers, keyed each blocks, CSS, and imported child components. Most
components share a leaf component and a TypeScript model. This is a synthetic
scaling test with repeated structure, not a representative sample of every app.
Both workloads use Svelte 5.57.0 and a strict bundler-resolution tsconfig.

<!-- BENCHMARKS:START -->
Median of 5 runs, measured 2026-09-05. svelte-check 4.7.6 vs svelte-check-rs 0.11.2.

Apple M4 Max · 48 GiB · macOS 27.0 · Node 24.20.0 · Bun 1.4.0.

| Workload | Scenario | svelte-check (TS6) | svelte-check (TS7 + incremental) | svelte-check-rs | Speedup vs TS7 |
| --- | --- | --- | --- | --- | --- |
| components-100 | Cold cache | 0.679 s | 0.436 s | 0.193 s | 2.3× |
| components-100 | Warm cache | 0.632 s | 0.241 s | 0.090 s | 2.7× |
| components-100 | Repeated TS edits | 0.670 s | 0.249 s | 0.096 s | 2.6× |
| components-500 | Cold cache | 1.284 s | 0.896 s | 0.413 s | 2.2× |
| components-500 | Warm cache | 1.299 s | 0.350 s | 0.156 s | 2.3× |
| components-500 | Repeated TS edits | 1.347 s | 0.374 s | 0.168 s | 2.2× |

Careswitch Web (private monorepo) has **different diagnostic results**: upstream TS7: **436 errors / 3 warnings**; Rust: **2 errors / 551 warnings**. These timings do not establish equivalent checking; no speedup claim is made for this workload.

| Workload | Scenario | svelte-check (TS7 + incremental) | svelte-check-rs |
| --- | --- | --- | --- |
| Careswitch Web | Cold cache | 16.789 s | 11.460 s |
| Careswitch Web | Warm cache | 3.140 s | 2.204 s |
| Careswitch Web | Repeated TS edits | 3.490 s | 2.408 s |
<!-- BENCHMARKS:END -->

## Method

- Compare the default `svelte-check`, `svelte-check --tsgo --incremental`, and
  `svelte-check-rs`. Upstream's `--tsgo` alone does **not** enable incremental
  caches. The experimental `--tsgo-experimental-api` mode is not measured.
  See [upstream's CLI documentation](https://github.com/sveltejs/language-tools/tree/master/packages/svelte-check).
- Each command receives `--workspace <path> --tsconfig ./tsconfig.json --output machine`.
  Node runs the upstream CLI directly; Rust runs the release binary directly.
  Package-manager launchers, installation, and compilation are outside the timer.
  `NO_COLOR=1` and `NODE_OPTIONS=--max-old-space-size=8192` are set for every run.
- Report the median of five complete process wall times, measured by Python's
  monotonic `perf_counter`. Capture stdout/stderr and wait for process exit.
  There is one untimed priming run per checker per scenario, plus an untimed
  value edit for the repeated-edit scenario. Rotate checker order
  each round; never run checkers concurrently.
- **Cold:** remove that checker's entire project cache before each measured run.
  Rust uses `node_modules/.cache/svelte-check-rs`; upstream uses `.svelte-check`
  (or `.svelte-kit/.svelte-check` in SvelteKit). This is a cold **checker** cache,
  not a cold OS page cache or a fresh dependency install. SvelteKit types, when
  applicable, are generated before measurement and retained.
  Rust's automatic Kit sync runs again when its sync manifest is removed with
  the cold cache; that time is included in its result.
- **Warm:** preserve the primed caches and leave sources unchanged.
- **Repeated TS edits:** prime with an extra typed numeric export, perform one
  untimed value edit, then change its value before each measured run, preserving
  the export's name and type. The public workload edits the shared `src/model.ts`;
  careswitch-web edits `src/lib/http-status-codes.ts`. This measures a small edit,
  not an API change that forces dependents to be rechecked. The initial addition
  of the export is outside the timer. The first edit after a fresh build was
  substantially slower for both tools on the private app, even with an unchanged
  export type. Its priming timing is retained in the raw data; the table measures
  subsequent edits after that initial invalidation.
- Public checkers run in separate generated workspaces with identical sources
  and shared pinned dependencies. Each has a local `node_modules` directory for
  isolated Rust caches. This also prevents either checker from discovering the
  other's generated virtual files.
- Before timing each public workload, inject a type error in a `.svelte` file,
  a type error in a `.ts` file, and an accessibility warning. Require two errors
  and one warning from every tool, then remove the probes and require zero
  diagnostics on every measured run. Record exit codes and hashes of sorted
  diagnostic records; abort on missing completion, failure records, unexpected
  exit status, or changed diagnostics within a scenario.
- Diagnostic probes establish that the main pipelines execute; they do not
  establish complete diagnostic parity. The checkers have different diagnostic
  coverage and transformation behavior. Rust's summary counts files with
  diagnostics, while upstream reports checked files; the raw `files` fields are
  therefore not comparable (Rust emits no file count on clean runs).

## Reproduce

Requires Rust 1.95+, Node, npm, Bun, and Python 3. Build and install before timing:

```sh
cargo build --release -p svelte-check-rs
npm ci --prefix benchmarks
python3 benchmarks/run.py --sizes 100 500 --runs 5
python3 -m unittest discover -s benchmarks -p 'test_*.py'
python3 benchmarks/publish.py
```

The npm lockfile pins transitive dependencies. Generated workspaces and full logs
stay in ignored `benchmarks/.work/`; results contain timings, counts, and hashes.
`publish.py` updates the marked README tables and the website's benchmark data
from the saved samples, so displayed ratios are calculated rather than hand-edited.

## Careswitch Web

This is a private application, measured separately from the reproducible fixtures.
Its source is not included (snapshot `4c4777935678cacb2313331a1c43ec84dacb856d`).
The snapshot has 626 `.svelte` and 2,340 `.ts` files
under `src`, plus scripts and shared monorepo packages. It uses Svelte 5.56.10,
SvelteKit 2.70.3, and Vite 8.2.2. Both native checkers use TypeScript 7.0.2.

Use a **disposable copy** of the monorepo: the runner removes checker caches and
temporarily edits the named file, restoring it in `finally`. Copy source files,
the installed dependency tree and workspace symlinks, generated Prisma clients,
the UI package's `dist`, and the app's local environment file. Keep relative
workspace paths intact. Run `bun run svelte-kit sync` in the copied app first.
Install the pinned TypeScript 6 and native TypeScript 7 packages into that copy
if it does not already provide them. The upstream CLI comes from this benchmark's
pinned install. Do not benchmark an incomplete copy with missing generated types.

```sh
python3 benchmarks/run.py \
  --workspace /absolute/path/to/disposable-monorepo/apps/careswitch-web \
  --label careswitch-web --edit-file src/lib/http-status-codes.ts \
  --tools tsgo rs --runs 5 --output benchmarks/careswitch-results.json
```

Full diagnostic logs remain local because they contain private paths and messages.
Only aggregate results are included in this repository.

The diagnostic mismatch needs separate investigation. Of Rust's 551 warnings,
535 are `state_referenced_locally`, which this app's configured `warningFilter`
suppresses in upstream; another 13 are Rust's heading-structure warnings.
Upstream's native mode reports many store-nullability and snippet-assignability
errors. Lower error counts or higher warning counts alone do not establish which
checker is correct. These discrepancies are not fixed or filtered out for the
benchmark.

During the sunset assessment, additional single diagnostic runs on the same
isolated copy reported **406 errors / 3 warnings** in both upstream's default
TypeScript 6 mode and its `--tsgo-experimental-api` mode. All 406 error locations
matched between those two modes. These were diagnostic checks, not repeated
timing benchmarks, and are not included in the tables. The discrepancy therefore
cannot be attributed solely to the older TS7 incremental mode. Its root causes
remain unresolved; the counts do not establish that any checker is correct or
that migration will preserve diagnostics.
