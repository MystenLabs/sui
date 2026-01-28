#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

"""Unit tests for release_notes.py"""

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


if __name__ == "__main__":
    unittest.main()
