#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

"""Unit tests for release_notes.py"""

import subprocess
import unittest
from unittest.mock import patch, MagicMock
from io import StringIO

from release_notes import parse_notes, Note


class TestParseNotes(unittest.TestCase):
    """Tests for the parse_notes function."""

    def test_parse_notes_empty_body(self):
        """Test parsing empty body returns empty dict."""
        notes = parse_notes("")
        self.assertEqual(notes, {})

    def test_parse_notes_none_body(self):
        """Test parsing None body returns empty dict."""
        notes = parse_notes(None)
        self.assertEqual(notes, {})

    def test_parse_notes_no_release_notes_section(self):
        """Test parsing body without release notes section."""
        body = "## Description\nSome description here."
        notes = parse_notes(body)
        self.assertEqual(notes, {})

    def test_parse_notes_with_checked_items(self):
        """Test parsing body with checked release notes."""
        body = """
## Description
Some description here.

### Release notes
- [x] Protocol: Added new feature X
- [ ] CLI: Not checked item
"""
        notes = parse_notes(body)

        self.assertEqual(len(notes), 2)
        self.assertTrue(notes["Protocol"].checked)
        self.assertEqual(notes["Protocol"].note, "Added new feature X")
        self.assertFalse(notes["CLI"].checked)
        self.assertEqual(notes["CLI"].note, "Not checked item")

    def test_parse_notes_case_insensitive_heading(self):
        """Test that release notes heading is case insensitive."""
        body = """
### RELEASE NOTES
- [x] Protocol: Test note
"""
        notes = parse_notes(body)

        self.assertEqual(len(notes), 1)
        self.assertTrue(notes["Protocol"].checked)

    def test_parse_notes_case_insensitive_checkbox(self):
        """Test that checkbox X is case insensitive."""
        body = """
### Release notes
- [X] Protocol: Uppercase X
- [x] CLI: Lowercase x
"""
        notes = parse_notes(body)

        self.assertTrue(notes["Protocol"].checked)
        self.assertTrue(notes["CLI"].checked)

    def test_parse_notes_with_space_checkbox(self):
        """Test parsing notes with space checkbox (standard unchecked format)."""
        body = """
### Release notes
- [ ] Protocol: Space checkbox unchecked
- [ ] CLI: Another unchecked
"""
        notes = parse_notes(body)

        self.assertFalse(notes["Protocol"].checked)
        self.assertFalse(notes["CLI"].checked)

    def test_parse_notes_preserves_note_content(self):
        """Test that note content is preserved correctly."""
        body = """
### Release notes
- [x] Protocol: This is a multi-word note with special chars: foo/bar
"""
        notes = parse_notes(body)

        self.assertEqual(
            notes["Protocol"].note,
            "This is a multi-word note with special chars: foo/bar",
        )

    def test_parse_notes_multiple_items(self):
        """Test parsing multiple release note items."""
        body = """
### Release notes
- [x] Protocol: Protocol change
- [x] JSON-RPC: RPC change
- [ ] GraphQL: Unchecked GraphQL
- [x] CLI: CLI update
"""
        notes = parse_notes(body)

        self.assertEqual(len(notes), 4)
        self.assertTrue(notes["Protocol"].checked)
        self.assertTrue(notes["JSON-RPC"].checked)
        self.assertFalse(notes["GraphQL"].checked)
        self.assertTrue(notes["CLI"].checked)


class TestPrHasReleaseNotes(unittest.TestCase):
    """Tests for pr_has_release_notes function."""

    @patch("release_notes.gql")
    def test_pr_has_release_notes_true(self, mock_gql):
        """Test that PR with checked release notes returns True."""
        from release_notes import pr_has_release_notes

        mock_gql.return_value = {
            "data": {
                "repository": {
                    "pullRequest": {
                        "body": "### Release notes\n- [x] Protocol: Some note"
                    }
                }
            }
        }

        result = pr_has_release_notes("123")

        self.assertTrue(result)
        mock_gql.assert_called_once()

    @patch("release_notes.gql")
    def test_pr_has_release_notes_false_unchecked(self, mock_gql):
        """Test that PR with only unchecked release notes returns False."""
        from release_notes import pr_has_release_notes

        mock_gql.return_value = {
            "data": {
                "repository": {
                    "pullRequest": {
                        "body": "### Release notes\n- [ ] Protocol: Some note"
                    }
                }
            }
        }

        result = pr_has_release_notes("123")

        self.assertFalse(result)
        mock_gql.assert_called_once()

    @patch("release_notes.gql")
    def test_pr_has_release_notes_false_no_body(self, mock_gql):
        """Test that PR with no body returns False."""
        from release_notes import pr_has_release_notes

        mock_gql.return_value = {
            "data": {"repository": {"pullRequest": {"body": None}}}
        }

        result = pr_has_release_notes("123")

        self.assertFalse(result)
        mock_gql.assert_called_once()

    @patch("release_notes.gql")
    def test_pr_has_release_notes_api_failure(self, mock_gql):
        """Test that API failure returns False."""
        from release_notes import pr_has_release_notes

        mock_gql.return_value = {"data": {}}

        result = pr_has_release_notes("123")

        self.assertFalse(result)
        mock_gql.assert_called_once()


class TestDoGetNotes(unittest.TestCase):
    """Tests for do_get_notes function."""

    @patch("release_notes.extract_notes_for_pr")
    def test_do_get_notes_output(self, mock_extract):
        """Test that do_get_notes outputs correctly formatted notes."""
        from release_notes import do_get_notes

        mock_extract.return_value = {
            "Protocol": Note(checked=True, note="Protocol note"),
            "CLI": Note(checked=False, note="Unchecked note"),
            "GraphQL": Note(checked=True, note="GraphQL note"),
        }

        with patch("sys.stdout", new=StringIO()) as mock_stdout:
            do_get_notes("123")
            output = mock_stdout.getvalue()

        self.assertIn("Protocol: Protocol note", output)
        self.assertIn("GraphQL: GraphQL note", output)
        self.assertNotIn("CLI:", output)


class TestExtractNotesForPr(unittest.TestCase):
    """Tests for extract_notes_for_pr function."""

    @patch("release_notes.gql")
    def test_extract_notes_for_pr_success(self, mock_gql):
        """Test extracting notes from PR body via GraphQL."""
        from release_notes import extract_notes_for_pr

        mock_gql.return_value = {
            "data": {
                "repository": {
                    "pullRequest": {
                        "body": "### Release notes\n- [x] Protocol: Test note"
                    }
                }
            }
        }

        notes = extract_notes_for_pr("123")

        self.assertEqual(len(notes), 1)
        self.assertTrue(notes["Protocol"].checked)
        self.assertEqual(notes["Protocol"].note, "Test note")
        mock_gql.assert_called_once()

    @patch("release_notes.gql")
    def test_extract_notes_for_pr_api_failure(self, mock_gql):
        """Test that API failure returns empty notes."""
        from release_notes import extract_notes_for_pr

        mock_gql.return_value = {"data": {}}

        notes = extract_notes_for_pr("123")

        self.assertEqual(len(notes), 0)
        mock_gql.assert_called_once()

    @patch("release_notes.gql")
    def test_extract_notes_for_pr_multiple_notes(self, mock_gql):
        """Test extracting multiple release notes from PR body."""
        from release_notes import extract_notes_for_pr

        mock_gql.return_value = {
            "data": {
                "repository": {
                    "pullRequest": {
                        "body": """## Description
Some PR description.

### Release notes
- [x] Protocol: Protocol change description
- [ ] CLI: Unchecked CLI note
- [x] GraphQL: GraphQL schema update
- [x] JSON-RPC: New RPC method added
"""
                    }
                }
            }
        }

        notes = extract_notes_for_pr("456")

        self.assertEqual(len(notes), 4)

        self.assertTrue(notes["Protocol"].checked)
        self.assertEqual(notes["Protocol"].note, "Protocol change description")

        self.assertFalse(notes["CLI"].checked)
        self.assertEqual(notes["CLI"].note, "Unchecked CLI note")

        self.assertTrue(notes["GraphQL"].checked)
        self.assertEqual(notes["GraphQL"].note, "GraphQL schema update")

        self.assertTrue(notes["JSON-RPC"].checked)
        self.assertEqual(notes["JSON-RPC"].note, "New RPC method added")


class TestPrBumpsProtocolVersion(unittest.TestCase):
    """Tests for pr_bumps_protocol_version function."""

    @patch("release_notes.GH_CLI_PATH", "/usr/bin/gh")
    @patch("release_notes.subprocess.check_output")
    def test_detects_protocol_version_bump(self, mock_check_output):
        """Test detection when MAX_PROTOCOL_VERSION is modified."""
        mock_check_output.return_value = """
diff --git a/crates/sui-protocol-config/src/lib.rs b/crates/sui-protocol-config/src/lib.rs
--- a/crates/sui-protocol-config/src/lib.rs
+++ b/crates/sui-protocol-config/src/lib.rs
@@ -25,7 +25,7 @@
 const MIN_PROTOCOL_VERSION: u64 = 1;
-const MAX_PROTOCOL_VERSION: u64 = 109;
+const MAX_PROTOCOL_VERSION: u64 = 110;
"""
        from release_notes import pr_bumps_protocol_version

        self.assertTrue(pr_bumps_protocol_version("123"))

    @patch("release_notes.GH_CLI_PATH", "/usr/bin/gh")
    @patch("release_notes.subprocess.check_output")
    def test_no_bump_when_file_not_changed(self, mock_check_output):
        """Test returns False when protocol config file not in diff."""
        mock_check_output.return_value = """
diff --git a/crates/sui-core/src/lib.rs b/crates/sui-core/src/lib.rs
--- a/crates/sui-core/src/lib.rs
+++ b/crates/sui-core/src/lib.rs
@@ -1,3 +1,4 @@
+// Some comment
"""
        from release_notes import pr_bumps_protocol_version

        self.assertFalse(pr_bumps_protocol_version("123"))

    @patch("release_notes.GH_CLI_PATH", "/usr/bin/gh")
    @patch("release_notes.subprocess.check_output")
    def test_no_bump_when_other_line_changed(self, mock_check_output):
        """Test returns False when lib.rs changed but not MAX_PROTOCOL_VERSION."""
        mock_check_output.return_value = """
diff --git a/crates/sui-protocol-config/src/lib.rs b/crates/sui-protocol-config/src/lib.rs
--- a/crates/sui-protocol-config/src/lib.rs
+++ b/crates/sui-protocol-config/src/lib.rs
@@ -100,6 +100,7 @@
+    some_new_config: true,
"""
        from release_notes import pr_bumps_protocol_version

        self.assertFalse(pr_bumps_protocol_version("123"))

    @patch("release_notes.GH_CLI_PATH", "/usr/bin/gh")
    @patch("release_notes.subprocess.check_output")
    def test_handles_gh_cli_failure(self, mock_check_output):
        """Test returns False when gh CLI fails."""
        mock_check_output.side_effect = subprocess.CalledProcessError(1, "gh")
        from release_notes import pr_bumps_protocol_version

        self.assertFalse(pr_bumps_protocol_version("123"))

    @patch("release_notes.GH_CLI_PATH", None)
    def test_returns_false_when_no_gh_cli(self):
        """Test returns False when gh CLI is not available."""
        from release_notes import pr_bumps_protocol_version

        self.assertFalse(pr_bumps_protocol_version("123"))


class TestDoCheckProtocolVersion(unittest.TestCase):
    """Tests for do_check with protocol version validation."""

    @patch("release_notes.pr_bumps_protocol_version")
    @patch("release_notes.extract_notes_for_pr")
    def test_fails_when_protocol_bump_without_note(self, mock_extract, mock_bumps):
        """Should fail when MAX_PROTOCOL_VERSION bumped but no Protocol note."""
        mock_bumps.return_value = True
        mock_extract.return_value = {
            "CLI": Note(checked=True, note="Some CLI change"),
        }
        from release_notes import do_check

        with self.assertRaises(SystemExit) as cm:
            with patch("sys.stdout", new=StringIO()):
                do_check("123")
        self.assertEqual(cm.exception.code, 1)

    @patch("release_notes.pr_bumps_protocol_version")
    @patch("release_notes.extract_notes_for_pr")
    def test_fails_when_protocol_bump_with_unchecked_note(self, mock_extract, mock_bumps):
        """Should fail when Protocol note exists but is not checked."""
        mock_bumps.return_value = True
        mock_extract.return_value = {
            "Protocol": Note(checked=False, note="Protocol change"),
        }
        from release_notes import do_check

        with self.assertRaises(SystemExit) as cm:
            with patch("sys.stdout", new=StringIO()):
                do_check("123")
        self.assertEqual(cm.exception.code, 1)

    @patch("release_notes.pr_bumps_protocol_version")
    @patch("release_notes.extract_notes_for_pr")
    def test_fails_when_protocol_bump_with_empty_note(self, mock_extract, mock_bumps):
        """Should fail when Protocol is checked but note is empty."""
        mock_bumps.return_value = True
        mock_extract.return_value = {
            "Protocol": Note(checked=True, note=""),
        }
        from release_notes import do_check

        with self.assertRaises(SystemExit) as cm:
            with patch("sys.stdout", new=StringIO()):
                do_check("123")
        self.assertEqual(cm.exception.code, 1)

    @patch("release_notes.pr_bumps_protocol_version")
    @patch("release_notes.extract_notes_for_pr")
    def test_passes_when_protocol_bump_with_valid_note(self, mock_extract, mock_bumps):
        """Should pass when Protocol bump has proper release note."""
        mock_bumps.return_value = True
        mock_extract.return_value = {
            "Protocol": Note(checked=True, note="Bump protocol version for feature X"),
        }
        from release_notes import do_check

        # Should not raise SystemExit
        do_check("123")

    @patch("release_notes.pr_bumps_protocol_version")
    @patch("release_notes.extract_notes_for_pr")
    def test_passes_when_no_protocol_bump(self, mock_extract, mock_bumps):
        """Should pass validation when no protocol version bump."""
        mock_bumps.return_value = False
        mock_extract.return_value = {}
        from release_notes import do_check

        # Should not raise SystemExit
        do_check("123")


class TestDoCheckEdgeCases(unittest.TestCase):
    """Tests for do_check edge cases: case sensitivity, whitespace, missing args."""

    def test_fails_when_pr_is_none(self):
        """Should fail with error when PR number is not provided."""
        from release_notes import do_check

        with self.assertRaises(SystemExit) as cm:
            with patch("sys.stderr", new=StringIO()):
                do_check(None)
        self.assertEqual(cm.exception.code, 1)

    def test_fails_when_pr_is_empty_string(self):
        """Should fail with error when PR number is empty string."""
        from release_notes import do_check

        with self.assertRaises(SystemExit) as cm:
            with patch("sys.stderr", new=StringIO()):
                do_check("")
        self.assertEqual(cm.exception.code, 1)

    @patch("release_notes.pr_bumps_protocol_version")
    @patch("release_notes.extract_notes_for_pr")
    def test_detects_case_mismatch(self, mock_extract, mock_bumps):
        """Should detect case mismatch and suggest correct case."""
        mock_bumps.return_value = False
        mock_extract.return_value = {
            "protocol": Note(checked=True, note="This is a valid release note"),
        }
        from release_notes import do_check

        with self.assertRaises(SystemExit) as cm:
            with patch("sys.stdout", new=StringIO()) as mock_stdout:
                do_check("123")
                output = mock_stdout.getvalue()
                self.assertIn("incorrect case", output)
                self.assertIn("Protocol", output)
        self.assertEqual(cm.exception.code, 1)


class TestParseNotesWhitespace(unittest.TestCase):
    """Tests for whitespace handling in parse_notes."""

    def test_strips_whitespace_from_impact_area(self):
        """Should strip whitespace from impact area names."""
        body = """
### Release notes
- [x]  Protocol : Note with extra spaces around impact area
"""
        notes = parse_notes(body)
        # The key should be "Protocol" not " Protocol " or "Protocol "
        self.assertIn("Protocol", notes)
        self.assertEqual(len(notes), 1)

    def test_handles_space_after_colon_variations(self):
        """Should handle notes with or without space after the colon."""
        body = """
### Release notes
- [x] Protocol: Note with space after colon
- [x] CLI:Note without space after colon
- [x] GraphQL:  Note with two spaces after colon
"""
        notes = parse_notes(body)

        self.assertEqual(len(notes), 3)
        # All should have properly trimmed notes
        self.assertEqual(notes["Protocol"].note, "Note with space after colon")
        self.assertEqual(notes["CLI"].note, "Note without space after colon")
        self.assertEqual(notes["GraphQL"].note, "Note with two spaces after colon")

    def test_parses_exact_pr_template_format(self):
        """Should correctly parse the exact format from PULL_REQUEST_TEMPLATE.md."""
        # This mirrors the exact format from .github/PULL_REQUEST_TEMPLATE.md
        body = """## Description

Some description here.

## Test plan

Some test plan.

---

## Release notes

Check each box that your changes affect.

- [x] Protocol: Added new protocol feature
- [ ] Nodes (Validators and Full nodes):
- [ ] gRPC:
- [x] JSON-RPC: New RPC method
- [ ] GraphQL:
- [x] CLI: New CLI command
- [ ] Rust SDK:
- [ ] Indexing Framework:
"""
        notes = parse_notes(body)

        # Should have 8 entries (all items from template)
        self.assertEqual(len(notes), 8)

        # Check that all known impact areas are parsed correctly
        self.assertIn("Protocol", notes)
        self.assertIn("Nodes (Validators and Full nodes)", notes)
        self.assertIn("gRPC", notes)
        self.assertIn("JSON-RPC", notes)
        self.assertIn("GraphQL", notes)
        self.assertIn("CLI", notes)
        self.assertIn("Rust SDK", notes)
        self.assertIn("Indexing Framework", notes)

        # Verify checked status
        self.assertTrue(notes["Protocol"].checked)
        self.assertFalse(notes["Nodes (Validators and Full nodes)"].checked)
        self.assertTrue(notes["JSON-RPC"].checked)
        self.assertTrue(notes["CLI"].checked)

        # Verify note content
        self.assertEqual(notes["Protocol"].note, "Added new protocol feature")
        self.assertEqual(notes["JSON-RPC"].note, "New RPC method")
        self.assertEqual(notes["CLI"].note, "New CLI command")


class TestDoCheckImprovements(unittest.TestCase):
    """Tests for do_check improvements: typo detection and short note warnings."""

    @patch("release_notes.pr_bumps_protocol_version")
    @patch("release_notes.extract_notes_for_pr")
    def test_suggests_correction_for_typo(self, mock_extract, mock_bumps):
        """Should suggest correction when impact area has a typo."""
        mock_bumps.return_value = False
        mock_extract.return_value = {
            "Protocal": Note(checked=True, note="This is a valid release note"),
        }
        from release_notes import do_check

        with self.assertRaises(SystemExit) as cm:
            with patch("sys.stdout", new=StringIO()) as mock_stdout:
                do_check("123")
                output = mock_stdout.getvalue()
                self.assertIn("Did you mean 'Protocol'", output)
        self.assertEqual(cm.exception.code, 1)

    @patch("release_notes.pr_bumps_protocol_version")
    @patch("release_notes.extract_notes_for_pr")
    def test_warns_about_short_notes(self, mock_extract, mock_bumps):
        """Should warn when a release note is very short."""
        mock_bumps.return_value = False
        mock_extract.return_value = {
            "Protocol": Note(checked=True, note="Short"),
        }
        from release_notes import do_check

        with patch("sys.stdout", new=StringIO()) as mock_stdout:
            # Should not raise SystemExit (warnings are non-fatal)
            do_check("123")
            output = mock_stdout.getvalue()
            self.assertIn("very short release note", output)

    @patch("release_notes.pr_bumps_protocol_version")
    @patch("release_notes.extract_notes_for_pr")
    def test_prints_success_message(self, mock_extract, mock_bumps):
        """Should print success message when check passes."""
        mock_bumps.return_value = False
        mock_extract.return_value = {
            "Protocol": Note(checked=True, note="This is a proper release note with enough detail"),
        }
        from release_notes import do_check

        with patch("sys.stdout", new=StringIO()) as mock_stdout:
            do_check("123")
            output = mock_stdout.getvalue()
            self.assertIn("check passed", output)

    @patch("release_notes.pr_bumps_protocol_version")
    @patch("release_notes.extract_notes_for_pr")
    def test_no_suggestion_for_completely_unknown_area(self, mock_extract, mock_bumps):
        """Should not suggest correction for completely unknown impact areas."""
        mock_bumps.return_value = False
        mock_extract.return_value = {
            "SomethingCompletelyDifferent": Note(checked=True, note="This is a valid release note"),
        }
        from release_notes import do_check

        with self.assertRaises(SystemExit) as cm:
            with patch("sys.stdout", new=StringIO()) as mock_stdout:
                do_check("123")
                output = mock_stdout.getvalue()
                self.assertIn("unfamiliar impact area", output)
                self.assertNotIn("Did you mean", output)
        self.assertEqual(cm.exception.code, 1)


if __name__ == "__main__":
    unittest.main()
