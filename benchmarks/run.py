#!/usr/bin/env python3
"""Sequential, validated CLI benchmarks. Run `python3 benchmarks/run.py --help`."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
RS_BINARY = ROOT / "target/release/svelte-check-rs"
TOOLS = {
    "default": ["node", str(HERE / "node_modules/svelte-check/bin/svelte-check")],
    "tsgo": ["node", str(HERE / "node_modules/svelte-check/bin/svelte-check"), "--tsgo", "--incremental"],
    "rs": [str(RS_BINARY)],
}
ENV = {**os.environ, "NO_COLOR": "1", "NODE_OPTIONS": "--max-old-space-size=8192"}


def output(command, cwd=ROOT):
    return subprocess.check_output(command, cwd=cwd, text=True, env=ENV).strip()


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def generate(size, tool):
    workspace = HERE / ".work" / f"components-{size}-{tool}"
    workspace.mkdir(parents=True, exist_ok=True)
    # Give Rust's nearest-node_modules cache resolution an isolated local home.
    (workspace / "node_modules").mkdir(exist_ok=True)
    write(workspace / "package.json", '{"private":true,"type":"module"}\n')
    write(workspace / "svelte.config.js", "export default {};\n")
    write(workspace / "tsconfig.json", json.dumps({
        "compilerOptions": {
            "target": "ES2022", "module": "ESNext", "moduleResolution": "bundler",
            "strict": True, "skipLibCheck": True, "allowJs": True, "checkJs": True,
            "allowImportingTsExtensions": True, "resolveJsonModule": True,
            "isolatedModules": True, "verbatimModuleSyntax": True, "noEmit": True,
        }, "include": ["src/**/*.ts", "src/**/*.svelte"],
    }, indent=2) + "\n")
    write(workspace / "src/model.ts", 'export interface Item { id: number; label: string; value: number }\nexport const scale: number = 2;\n')
    write(workspace / "src/Leaf.svelte", '''<script lang="ts">
  let { label, value }: { label: string; value: number } = $props();
</script>
<span>{label}: {value.toFixed(2)}</span>
''')
    for i in range(size - 1):
        write(workspace / f"src/data{i}.ts", f'''import type {{ Item }} from './model';
export const items: Item[] = [{{ id: {i}, label: 'Item {i}', value: {i + 1} }}];
''')
        write(workspace / f"src/Component{i}.svelte", f'''<script lang="ts">
  import Leaf from './Leaf.svelte';
  import {{ items }} from './data{i}';
  import {{ scale }} from './model';
  let {{ title = 'Component {i}' }}: {{ title?: string }} = $props();
  let count = $state(0);
  let total = $derived(count * scale);
</script>
{{#snippet heading(text: string)}}<h2>{{text}}</h2>{{/snippet}}
<section>
  {{@render heading(title)}}
  <button onclick={{() => count += 1}}>Count {{total}}</button>
  {{#each items as item (item.id)}}
    <Leaf label={{item.label}} value={{item.value + total}} />
  {{/each}}
</section>
<style>section {{ padding: 1rem; }} button {{ color: navy; }}</style>
''')
    return workspace


def clear_cache(workspace, tool):
    paths = ([workspace / "node_modules/.cache/svelte-check-rs"] if tool == "rs" else
             [workspace / ".svelte-check", workspace / ".svelte-kit/.svelte-check"])
    for path in paths:
        if not path.resolve().is_relative_to(workspace.resolve()):
            raise ValueError(f"Refusing to remove cache outside the disposable workspace: {path}")
        if path.exists():
            shutil.rmtree(path)


def invoke(workspace, tool, log):
    command = TOOLS[tool] + ["--workspace", str(workspace), "--tsconfig", "./tsconfig.json", "--output", "machine"]
    start = time.perf_counter()
    result = subprocess.run(command, cwd=workspace, env=ENV, capture_output=True, text=True, timeout=300)
    elapsed = time.perf_counter() - start
    combined = result.stdout + "\n" + result.stderr
    write(log, combined)
    pattern = (r"svelte-check-rs found (\d+) errors? and (\d+) warnings?(?: in (\d+) files?)?" if tool == "rs" else
               r"COMPLETED (\d+) FILES (\d+) ERRORS (\d+) WARNINGS")
    match = re.search(pattern, result.stdout)
    if not match or "FAILURE" in combined or result.returncode not in (0, 1):
        raise RuntimeError(f"{tool} did not complete a diagnostic run; inspect {log}")
    counts = [int(n) if n is not None else None for n in match.groups()]
    files, errors, warnings = (counts[2], counts[0], counts[1]) if tool == "rs" else counts
    if result.returncode != (1 if errors else 0):
        raise RuntimeError(f"{tool}: unexpected exit status {result.returncode}; inspect {log}")
    # Timestamps and order vary across processes; preserve diagnostic identity.
    diagnostics = sorted(re.sub(r"^\d+ ", "", line) for line in result.stdout.splitlines()
                         if re.match(r"^(?:\d+ )?(ERROR|WARNING) ", line))
    return {"seconds": elapsed, "files": files, "errors": errors, "warnings": warnings,
            "exit_code": result.returncode,
            "diagnostics_sha256": hashlib.sha256("\n".join(diagnostics).encode()).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sizes", nargs="+", type=int, default=[100, 500])
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output", type=Path, default=HERE / "results.json")
    parser.add_argument("--workspace", type=Path, help="Existing DISPOSABLE workspace; caches will be removed")
    parser.add_argument("--edit-file", help="Existing file relative to workspace for iterative runs")
    parser.add_argument("--label", default="external")
    parser.add_argument("--tools", nargs="+", choices=list(TOOLS), default=list(TOOLS))
    parser.add_argument("--scenarios", nargs="+", choices=["cold", "warm", "iterative"], default=["cold", "warm", "iterative"])
    args = parser.parse_args()
    for tool in list(TOOLS):
        if tool not in args.tools:
            del TOOLS[tool]
    if args.runs < 2 or any(size < 2 for size in args.sizes):
        parser.error("Use at least two runs and two components")
    if args.workspace and not args.edit_file:
        parser.error("--workspace requires --edit-file")
    if args.workspace:
        edit_path = (args.workspace / args.edit_file).resolve()
        if not edit_path.is_relative_to(args.workspace.resolve()) or edit_path.suffix != ".ts":
            parser.error("--edit-file must be a TypeScript file inside the disposable workspace")
    packages = {}
    for name in ("svelte-check", "svelte", "typescript", "@typescript/native"):
        packages[name] = json.loads((HERE / "node_modules" / name / "package.json").read_text())["version"]
    report = {
        "date": time.strftime("%Y-%m-%d"), "platform": platform.platform(),
        "cpu": output(["sysctl", "-n", "machdep.cpu.brand_string"]) if platform.system() == "Darwin" else platform.processor(),
        "logical_cpus": os.cpu_count(), "node": output(["node", "--version"]),
        "bun": output(["bun", "--version"]), "rust": output(["rustc", "--version"]),
        "rs_version": output([str(RS_BINARY), "--version"]), "rs_commit": output(["git", "rev-parse", "HEAD"]),
        "node_options": ENV["NODE_OPTIONS"],
        "iterative_edit": "Repeated numeric export value changes after one untimed edit; preserve name and type",
        "priming_runs": {"cold": 1, "warm": 1, "iterative": 2},
        "packages": packages, "runs": args.runs, "statistic": "median",
        "commands": {key: [Path(cmd[0]).name, *[str(x).replace(str(ROOT), "<repo>") for x in cmd[1:]],
                           "--workspace", "<workspace>", "--tsconfig", "./tsconfig.json", "--output", "machine"] for key, cmd in TOOLS.items()},
        "workloads": {},
    }
    if platform.system() == "Darwin":
        report["memory_bytes"] = int(output(["sysctl", "-n", "hw.memsize"]))
        report["macos"] = output(["sw_vers", "-productVersion"])
    workspaces = [(args.label, args.workspace.resolve())] if args.workspace else [
        (f"components-{size}", {tool: generate(size, tool) for tool in TOOLS}) for size in args.sizes]
    for label, paths in workspaces:
        paths = paths if isinstance(paths, dict) else {tool: paths for tool in TOOLS}
        workspace = next(iter(paths.values()))
        logs = HERE / ".work/logs" / label
        edits = {tool: path / (args.edit_file or "src/model.ts") for tool, path in paths.items()}
        original = next(iter(edits.values())).read_text()
        workload = {"scenarios": {}, "probe": {}, "priming": {}}
        workload["packages"] = json.loads(output([
            "node", "--input-type=module", "-e",
            "import {createRequire} from 'node:module'; const r=createRequire(process.cwd()+'/package.json');"
            "const names=['svelte','typescript','@typescript/native','@sveltejs/kit','vite'];"
            "console.log(JSON.stringify(Object.fromEntries(names.flatMap(n=>{try{return [[n,r(n+'/package.json').version]]}catch{return []}}))))",
        ], cwd=workspace))
        report["workloads"][label] = workload
        files = [p for p in (workspace / "src").rglob("*") if p.suffix in (".svelte", ".ts", ".js")]
        workload["source_files"] = {"svelte": sum(p.suffix == ".svelte" for p in files),
                                    "ts_js": sum(p.suffix in (".ts", ".js") for p in files),
                                    "lines": sum(len(p.read_text().splitlines()) for p in files)}
        # On public fixtures prove both TS and Svelte checking execute before timing.
        if not args.workspace:
            for tool, path in paths.items():
                probe = path / "src/Probe.svelte"
                write(probe, '<script lang="ts">let value: number = "wrong";</script>\n<img src="test.png">\n<p>{value}</p>\n')
                edits[tool].write_text(original + '\nexport const invalid: number = "wrong";\n')
                try:
                    clear_cache(path, tool)
                    result = invoke(path, tool, logs / f"probe-{tool}.log")
                    if result["errors"] != 2 or result["warnings"] != 1:
                        raise RuntimeError(f"{tool}: diagnostic probe failed: {result}")
                    probe_output = (logs / f"probe-{tool}.log").read_text()
                    if not all(text in probe_output for text in ("src/Probe.svelte", "src/model.ts", "not assignable", "alt")):
                        raise RuntimeError(f"{tool}: diagnostic probe did not report the expected sources")
                    workload["probe"][tool] = result
                finally:
                    probe.unlink()
                    edits[tool].write_text(original)
        try:
            for scenario in args.scenarios:
                measurements = {tool: [] for tool in TOOLS}
                workload["scenarios"][scenario] = measurements
                for edit in edits.values():
                    # Prime with the same export shape used in every measured edit.
                    # Adding an export once then changing its value would mix API
                    # invalidation and implementation-only edits in one median.
                    edit.write_text(original + "\nexport const benchmarkRevision: number = -1;\n"
                                    if scenario == "iterative" else original)
                # One untimed priming run per tool for each scenario.
                expected = {}
                priming = workload["priming"][scenario] = {}
                for tool in TOOLS:
                    # External tools share one disposable workspace; restore the
                    # same baseline before priming each tool's independent cache.
                    if scenario == "iterative":
                        edits[tool].write_text(original + "\nexport const benchmarkRevision: number = -1;\n")
                    clear_cache(paths[tool], tool)
                    expected[tool] = invoke(paths[tool], tool, logs / f"{scenario}-{tool}-warmup.log")
                    priming[tool] = [expected[tool]]
                    if scenario == "iterative":
                        edits[tool].write_text(original + "\nexport const benchmarkRevision: number = -2;\n")
                        changed = invoke(paths[tool], tool, logs / f"{scenario}-{tool}-edit-warmup.log")
                        if any(changed[field] != expected[tool][field] for field in ("errors", "warnings", "diagnostics_sha256")):
                            raise RuntimeError(f"{tool}: diagnostics changed during the priming edit")
                        priming[tool].append(changed)
                    if not args.workspace and (expected[tool]["errors"] or expected[tool]["warnings"]):
                        raise RuntimeError(f"{tool}: public workload must have zero diagnostics")
                for run in range(args.runs):
                    # Rotate order to reduce ordering bias; never run checkers concurrently.
                    order = list(TOOLS)
                    order = order[run % len(order):] + order[:run % len(order)]
                    for tool in order:
                        if scenario == "cold":
                            clear_cache(paths[tool], tool)
                        elif scenario == "iterative":
                            # Same content change for each tool's independent cache.
                            edits[tool].write_text(original + f"\nexport const benchmarkRevision: number = {run};\n")
                        result = invoke(paths[tool], tool, logs / f"{scenario}-{tool}-{run + 1}.log")
                        for field in ("errors", "warnings", "diagnostics_sha256"):
                            if result[field] != expected[tool][field]:
                                raise RuntimeError(f"{tool}: diagnostics changed during {scenario}; inspect {logs}")
                        measurements[tool].append(result)
                        print(f"{label} {scenario} {run + 1}/{args.runs} {tool}: {result['seconds']:.3f}s ({result['errors']} errors, {result['warnings']} warnings)", flush=True)
                workload.setdefault("medians", {})[scenario] = {
                    tool: statistics.median(r["seconds"] for r in runs) for tool, runs in measurements.items()}
                write(args.output, json.dumps(report, indent=2) + "\n")
        finally:
            for edit in edits.values():
                edit.write_text(original)


if __name__ == "__main__":
    main()
