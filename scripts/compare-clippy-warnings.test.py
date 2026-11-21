import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("compare-clippy-warnings.py").resolve()
MANIFEST = SCRIPT.parents[1] / "src-tauri/Cargo.toml"


def warning(path, line):
    return {
        "reason": "compiler-message",
        "manifest_path": str(MANIFEST),
        "message": {
            "level": "warning",
            "code": {"code": "dead_code"},
            "message": "unused function",
            "spans": [{"is_primary": True, "file_name": path, "line_start": line}],
        },
    }


class WarningComparison(unittest.TestCase):
    def compare(self, baseline, candidate, success=True):
        with tempfile.TemporaryDirectory() as temp:
            paths = []
            for name, events, ok in [("base", baseline, True), ("task", candidate, success)]:
                path = Path(temp, name)
                events = [*events, {"reason": "build-finished", "success": ok}]
                path.write_text("\n".join(map(json.dumps, events)), encoding="utf-8")
                paths.append(path)
            return subprocess.run([sys.executable, str(SCRIPT), *map(str, paths)], capture_output=True, text=True)

    def test_ignores_locations_and_counts_duplicate_target_diagnostics_once(self):
        old = warning("src/lib.rs", 1)
        shifted = warning("src/lib.rs", 42)
        result = self.compare([old], [shifted, shifted])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("baseline=1 candidate=1 added=0 knowledge=0", result.stdout)

    def test_rejects_new_file_and_a_preexisting_knowledge_warning(self):
        warning_in_store = warning("src/knowledge/store.rs", 7)
        result = self.compare([warning_in_store], [warning_in_store, warning("src/lib.rs", 9)])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("added=1 knowledge=1", result.stdout)

    def test_failed_build_is_never_a_clean_comparison(self):
        self.assertNotEqual(self.compare([], [], success=False).returncode, 0)


if __name__ == "__main__":
    unittest.main()
