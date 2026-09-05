import json
from pathlib import Path
import tempfile
import unittest

import publish


class PublicationTests(unittest.TestCase):
    def report(self):
        return {"runs": 3, "workloads": {"example": {"scenarios": {
            key: {tool: [{"seconds": value} for value in values]
                  for tool, values in {"tsgo": [2, 10, 3], "rs": [1, 5, 1.5]}.items()}
            for key in publish.SCENARIOS
        }, "medians": {"warm": {"tsgo": 999, "rs": 0.01}}}}}

    def load(self, report):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "results.json"
            path.write_text(json.dumps(report))
            return publish.load_report(path)

    def test_recalculates_medians_from_samples(self):
        result = self.load(self.report())
        self.assertEqual(result["workloads"]["example"]["medians"]["warm"], {"tsgo": 3, "rs": 1.5})

    def test_rejects_incomplete_or_invalid_measurements(self):
        for value in ([], [{"seconds": 1}], [{"seconds": float("nan")}] * 3):
            report = self.report()
            report["workloads"]["example"]["scenarios"]["warm"]["rs"] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                self.load(report)

    def test_qualified_table_does_not_claim_speedup(self):
        workloads = self.load(self.report())["workloads"]
        table = publish.table(workloads, include_default=False, ratios=False)
        self.assertNotIn("Speedup", table)
        self.assertNotIn("×", table)
        self.assertIn("3.000 s", table)


if __name__ == "__main__":
    unittest.main()
