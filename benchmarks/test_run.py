"""Check that the timing harness rejects failed or incomplete checker runs."""

from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import run


class HarnessTests(unittest.TestCase):
    def invoke(self, tool, stdout, code=0, stderr=""):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            result = subprocess.CompletedProcess([], code, stdout, stderr)
            with patch.object(run.subprocess, "run", return_value=result):
                return run.invoke(path, tool, path / "log")

    def test_completed_zero_diagnostic_runs(self):
        upstream = self.invoke("tsgo", "123 COMPLETED 100 FILES 0 ERRORS 0 WARNINGS\n")
        rust = self.invoke("rs", "svelte-check-rs found 0 errors and 0 warnings\n")
        self.assertEqual(upstream["errors"], 0)
        self.assertEqual(upstream["diagnostics_sha256"], rust["diagnostics_sha256"])
        # Rust's summary only counts files with problems, not all checked files.
        self.assertIsNone(rust["files"])

    def test_rejects_missing_summary_failure_and_wrong_exit(self):
        for stdout, code, stderr in [
            ("", 0, "native compiler failed"),
            ("123 COMPLETED 100 FILES 0 ERRORS 0 WARNINGS", 1, ""),
            ("123 COMPLETED 100 FILES 0 ERRORS 0 WARNINGS", 0, "FAILURE closed"),
            ("123 COMPLETED 100 FILES 2 ERRORS 0 WARNINGS", 0, ""),
        ]:
            with self.subTest(stdout=stdout, code=code), self.assertRaises(RuntimeError):
                self.invoke("tsgo", stdout, code, stderr)

    def test_diagnostic_identity_ignores_timestamps_and_order(self):
        a = '123 ERROR "a.ts" 1:1 "bad"\n456 WARNING "b.svelte" 2:1 "alt"\n'
        b = '987 WARNING "b.svelte" 2:1 "alt"\n999 ERROR "a.ts" 1:1 "bad"\n'
        summary = "999 COMPLETED 2 FILES 1 ERRORS 1 WARNINGS"
        self.assertEqual(self.invoke("tsgo", a + summary, 1)["diagnostics_sha256"],
                         self.invoke("tsgo", b + summary, 1)["diagnostics_sha256"])
        self.assertNotEqual(self.invoke("tsgo", a + summary, 1)["diagnostics_sha256"],
                            self.invoke("tsgo", a.replace("bad", "different") + summary, 1)["diagnostics_sha256"])

    def test_cold_cache_removal_preserves_dependencies_and_kit_types(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory)
            for name in ("node_modules/.cache/svelte-check-rs/entry", ".svelte-check/entry",
                         ".svelte-kit/.svelte-check/entry", ".svelte-kit/ambient.d.ts", "node_modules/svelte/package.json"):
                run.write(workspace / name, "test")
            for tool in ("tsgo", "rs"):
                run.clear_cache(workspace, tool)
            self.assertFalse((workspace / "node_modules/.cache/svelte-check-rs").exists())
            self.assertFalse((workspace / ".svelte-check").exists())
            self.assertFalse((workspace / ".svelte-kit/.svelte-check").exists())
            self.assertTrue((workspace / ".svelte-kit/ambient.d.ts").exists())
            self.assertTrue((workspace / "node_modules/svelte/package.json").exists())

    def test_refuses_to_clear_a_shared_dependency_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "workspace"
            workspace.mkdir()
            run.write(root / "shared/.cache/svelte-check-rs/entry", "keep")
            (workspace / "node_modules").symlink_to(root / "shared", target_is_directory=True)
            with self.assertRaises(ValueError):
                run.clear_cache(workspace, "rs")
            self.assertTrue((root / "shared/.cache/svelte-check-rs/entry").exists())


if __name__ == "__main__":
    unittest.main()
