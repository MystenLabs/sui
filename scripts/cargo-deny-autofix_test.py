#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

"""Exercise the autofix script with controlled Cargo diagnostics and updates."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(
    os.environ.get("AUTOFIX_SCRIPT", Path(__file__).with_name("cargo-deny-autofix.sh"))
).resolve()

MOCK_CARGO = r'''
import json
import os
import pathlib
import sys
root = pathlib.Path(os.environ["FIXTURE_ROOT"])
fixture = json.loads((root / "fixture.json").read_text())
if sys.argv[1] == "deny":
    workspace = str(pathlib.Path(sys.argv[sys.argv.index("--manifest-path") + 1]).parent)
    marker = root / ("scanned-" + workspace.replace("/", "-"))
    findings = fixture["after" if marker.exists() else "before"]
    if isinstance(findings, dict):
        findings = findings[workspace]
    marker.touch()
    for advisory, package, version in findings:
        print(json.dumps({"type": "diagnostic", "fields": {
            "severity": "error", "advisory": {"id": advisory},
            "graphs": [{"Krate": {"name": package, "version": version}}]
        }}), file=sys.stderr)
    print(json.dumps({"type": "summary"}), file=sys.stderr)
    sys.exit(bool(findings))
if sys.argv[1] == "update":
    package, old = sys.argv[sys.argv.index("-p") + 1].split("@")
    new = fixture["updates"].get(package + "@" + old)
    if new:
        print(f"    Updating {package} v{old} -> v{new}", file=sys.stderr)
    sys.exit(0)
raise SystemExit("unexpected cargo invocation")
'''


class AutofixReportingTests(unittest.TestCase):
    def run_case(self, before, after, updates, workspaces=(".",)):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "bin").mkdir()
            (root / "fixture.json").write_text(json.dumps({
                "before": before, "after": after, "updates": updates,
            }))
            cargo = root / "bin" / "cargo"
            cargo.write_text(f"#!{sys.executable}\n" + MOCK_CARGO)
            cargo.chmod(0o755)
            curl = root / "bin" / "curl"
            curl.write_text("#!/bin/sh\nexit 1\n")
            curl.chmod(0o755)
            output = root / "output"
            github_output = root / "github-output"
            env = dict(
                os.environ,
                FIXTURE_ROOT=str(root),
                OUT_DIR=str(output),
                GITHUB_OUTPUT=str(github_output),
                PATH=str(root / "bin") + os.pathsep + os.environ["PATH"],
            )
            result = subprocess.run(
                ["bash", str(SCRIPT), *workspaces], cwd=root, env=env,
                text=True, capture_output=True, timeout=15,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            return (
                (output / "fixed.md").read_text(),
                (output / "remaining.md").read_text(),
                github_output.read_text(),
            )

    def test_updated_version_still_affected(self):
        fixed, remaining, outputs = self.run_case(
            [["RUSTSEC-TEST-A", "example", "1.0.0"]],
            [["RUSTSEC-TEST-A", "example", "1.0.1"]], {"example@1.0.0": "1.0.1"})
        self.assertEqual(fixed, "")
        self.assertIn("RUSTSEC-TEST-A", remaining)
        self.assertIn("has_fixes=false", outputs)
        self.assertIn("has_remaining=true", outputs)

    def test_update_clears_advisory(self):
        fixed, remaining, outputs = self.run_case(
            [["RUSTSEC-TEST-A", "example", "1.0.0"]], [], {"example@1.0.0": "1.0.1"})
        self.assertIn("1.0.0 → 1.0.1", fixed)
        self.assertEqual(remaining, "")
        self.assertIn("has_fixes=true", outputs)
        self.assertIn("has_remaining=false", outputs)

    def test_advisory_on_version_changed_after_recorded_bump(self):
        fixed, remaining, _ = self.run_case(
            [["RUSTSEC-TEST-A", "example", "1.0.0"],
             ["RUSTSEC-TEST-B", "other", "2.0.0"]],
            [["RUSTSEC-TEST-A", "example", "1.0.2"]],
            {"example@1.0.0": "1.0.1", "other@2.0.0": "2.0.1"})
        self.assertNotIn("RUSTSEC-TEST-A", fixed)
        self.assertIn("RUSTSEC-TEST-B", fixed)
        self.assertIn("1.0.2", remaining)

    def test_shared_package_only_one_advisory_cleared(self):
        fixed, remaining, _ = self.run_case(
            [["RUSTSEC-TEST-A", "example", "1.0.0"],
             ["RUSTSEC-TEST-B", "example", "1.0.0"]],
            [["RUSTSEC-TEST-B", "example", "1.0.1"]], {"example@1.0.0": "1.0.1"})
        self.assertIn("RUSTSEC-TEST-A", fixed)
        self.assertNotIn("RUSTSEC-TEST-B", fixed)
        self.assertIn("RUSTSEC-TEST-B", remaining)

    def test_advisory_on_another_locked_version_is_not_fully_fixed(self):
        fixed, remaining, _ = self.run_case(
            [["RUSTSEC-TEST-A", "example", "1.0.0"],
             ["RUSTSEC-TEST-A", "example", "2.0.0"]],
            [["RUSTSEC-TEST-A", "example", "2.0.0"]], {"example@1.0.0": "1.0.1"})
        self.assertEqual(fixed, "")
        self.assertIn("2.0.0", remaining)

    def test_all_affected_versions_cleared(self):
        fixed, remaining, _ = self.run_case(
            [["RUSTSEC-TEST-A", "example", "1.0.0"],
             ["RUSTSEC-TEST-A", "example", "2.0.0"]], [],
            {"example@1.0.0": "1.0.1", "example@2.0.0": "2.0.1"})
        self.assertIn("1.0.0 → 1.0.1", fixed)
        self.assertIn("2.0.0 → 2.0.1", fixed)
        self.assertEqual(remaining, "")

    def test_original_affected_version_still_present(self):
        fixed, remaining, _ = self.run_case(
            [["RUSTSEC-TEST-A", "example", "1.0.0"]],
            [["RUSTSEC-TEST-A", "example", "1.0.0"]], {"example@1.0.0": "1.0.1"})
        self.assertEqual(fixed, "")
        self.assertIn("1.0.0", remaining)

    def test_no_compatible_update(self):
        finding = [["RUSTSEC-TEST-A", "example", "1.0.0"]]
        fixed, remaining, _ = self.run_case(finding, finding, {})
        self.assertEqual(fixed, "")
        self.assertIn("RUSTSEC-TEST-A", remaining)

    def test_clean_scan(self):
        fixed, remaining, outputs = self.run_case([], [], {})
        self.assertEqual((fixed, remaining), ("", ""))
        self.assertIn("has_fixes=false", outputs)
        self.assertIn("has_remaining=false", outputs)

    def test_remaining_findings_are_scoped_to_each_workspace(self):
        before = [["RUSTSEC-TEST-A", "example", "1.0.0"]]
        for order in ((".", "external-crates/move"), ("external-crates/move", ".")):
            with self.subTest(order=order):
                fixed, remaining, outputs = self.run_case(
                    {".": before, "external-crates/move": before},
                    {".": [["RUSTSEC-TEST-A", "example", "1.0.1"]],
                     "external-crates/move": []},
                    {"example@1.0.0": "1.0.1"}, workspaces=order,
                )
                self.assertEqual(len(fixed.splitlines()), 1)
                self.assertIn("in `external-crates/move`", fixed)
                self.assertEqual(len(remaining.splitlines()), 1)
                self.assertIn("in `.`", remaining)
                self.assertIn("has_fixes=true", outputs)
                self.assertIn("has_remaining=true", outputs)


if __name__ == "__main__":
    unittest.main()
