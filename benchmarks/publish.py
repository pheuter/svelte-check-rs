#!/usr/bin/env python3
"""Regenerate documentation tables and website data from recorded samples."""

import json
import math
from pathlib import Path
import statistics

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
SCENARIOS = {"cold": "Cold cache", "warm": "Warm cache", "iterative": "Repeated TS edits"}


def environment(report):
    parts = [report["cpu"] or report["platform"]]
    if report.get("memory_bytes"):
        parts.append(f"{report['memory_bytes'] / 1024**3:g} GiB")
    parts.extend([f"macOS {report['macos']}" if "macos" in report else report["platform"],
                  f"Node {report['node'].removeprefix('v')}", f"Bun {report['bun']}"])
    return " · ".join(parts)


def table(workloads, include_default=True, ratios=True):
    columns = ["Workload", "Scenario"]
    if include_default:
        columns.append("svelte-check (TS6)")
    columns.extend(["svelte-check (TS7 + incremental)", "svelte-check-rs"])
    if ratios:
        columns.append("Speedup vs TS7")
    lines = ["| " + " | ".join(columns) + " |", "| " + " | ".join(["---"] * len(columns)) + " |"]
    for name, workload in workloads.items():
        for key, label in SCENARIOS.items():
            times = workload["medians"][key]
            cells = [name, label]
            if include_default:
                cells.append(f"{times['default']:.3f} s")
            cells.extend([f"{times['tsgo']:.3f} s", f"{times['rs']:.3f} s"])
            if ratios:
                cells.append(f"{times['tsgo'] / times['rs']:.1f}×")
            lines.append("| " + " | ".join(cells) + " |")
    return "\n".join(lines)


def replace_block(path, text):
    source = path.read_text()
    start, rest = source.split("<!-- BENCHMARKS:START -->", 1)
    _, end = rest.split("<!-- BENCHMARKS:END -->", 1)
    path.write_text(start + "<!-- BENCHMARKS:START -->\n" + text + "\n<!-- BENCHMARKS:END -->" + end)


def load_report(path):
    report = json.loads(path.read_text())
    for workload in report["workloads"].values():
        for scenario in SCENARIOS:
            measurements = workload["scenarios"].get(scenario, {})
            if not {"rs", "tsgo"}.issubset(measurements):
                raise ValueError(f"{path}: incomplete {scenario} measurements")
            for samples in measurements.values():
                if len(samples) != report["runs"] or any(
                    not math.isfinite(r["seconds"]) or r["seconds"] <= 0 for r in samples
                ):
                    raise ValueError(f"{path}: invalid samples in {scenario}")
        # Always derive the displayed medians from individual samples.
        workload["medians"] = {
            scenario: {tool: statistics.median(r["seconds"] for r in samples)
                       for tool, samples in measurements.items()}
            for scenario, measurements in workload["scenarios"].items()
        }
    return report


def main():
    public = load_report(HERE / "results.json")
    for workload in public["workloads"].values():
        if any(r["errors"] or r["warnings"] for measurements in workload["scenarios"].values()
               for samples in measurements.values() for r in samples):
            raise ValueError("Public benchmark speedups require clean diagnostic runs")
    real_path = HERE / "careswitch-results.json"
    real = load_report(real_path) if real_path.exists() else None
    introduction = (f"Median of {public['runs']} runs, measured {public['date']}. "
                    f"svelte-check {public['packages']['svelte-check']} vs {public['rs_version']}.\n\n"
                    f"{environment(public)}.\n\n")
    full = introduction + table(public["workloads"])
    short = introduction + table({"500 components (synthetic)": public["workloads"]["components-500"]})
    short += "\n\nAll timed public-fixture runs reported **0 errors and 0 warnings**."
    site = {}
    for report in [public, *([real] if real else [])]:
        for name, workload in report["workloads"].items():
            is_public = report is public
            diagnostics = {tool: {field: runs[0][field] for field in ("errors", "warnings")}
                           for tool, runs in workload["scenarios"]["warm"].items()}
            if not is_public:
                counts = (f"upstream TS7: **{diagnostics['tsgo']['errors']} errors / {diagnostics['tsgo']['warnings']} warnings**; "
                          f"Rust: **{diagnostics['rs']['errors']} errors / {diagnostics['rs']['warnings']} warnings**")
                caveat = ("Careswitch Web (private monorepo) has **different diagnostic results**: " + counts +
                          ". These timings do not establish equivalent checking; no speedup claim is made for this workload.")
                short += "\n\n" + caveat + "\n\n" + table({"Careswitch Web": workload}, False, False)
                full += "\n\n" + caveat + "\n\n" + table({"Careswitch Web": workload}, False, False)
            source = workload["source_files"]
            title = f"{source['svelte']} components · synthetic" if is_public else "Careswitch Web · private"
            site[name] = {
                "title": title, "runs": report["runs"], "date": report["date"],
                "environment": environment(report),
                "checkers": f"svelte-check {report['packages']['svelte-check']} · {report['rs_version']}",
                "source": f"{source['svelte']:,} .svelte · {source['ts_js']:,} .ts/.js · {source['lines']:,} source lines",
                "packages": workload["packages"], "showSpeedup": is_public,
                "note": ("All three tools: 0 errors, 0 warnings. Synthetic workload; results vary by project."
                         if is_public else f"Upstream TS7: {diagnostics['tsgo']['errors']} errors / {diagnostics['tsgo']['warnings']} warnings. Rust: {diagnostics['rs']['errors']} errors / {diagnostics['rs']['warnings']} warnings. Timings do not establish equivalent checking."),
                "scenarios": {key: {"label": SCENARIOS[key],
                                     **{("svelte" if tool == "default" else tool): seconds
                                        for tool, seconds in times.items()}}
                              for key, times in workload["medians"].items()},
            }
    replace_block(ROOT / "README.md", short)
    replace_block(HERE / "README.md", full)
    (ROOT / "docs/benchmark-data.js").write_text(
        "// Generated by benchmarks/publish.py from recorded wall-clock samples.\n"
        "const BENCHMARK_WORKLOADS = " + json.dumps(site, indent=2) + ";\n")


if __name__ == "__main__":
    main()
