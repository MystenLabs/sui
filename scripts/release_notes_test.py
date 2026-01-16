#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

"""Tests for release_notes.py"""

import unittest
from unittest.mock import patch, MagicMock
import json
import sys
import io

from release_notes import (
    parse_notes,
    Note,
    NOTE_ORDER,
    INTERESTING_DIRECTORIES,
)


class TestParseNotes(unittest.TestCase):
    """Tests for the parse_notes function."""

    def test_parse_notes_with_checked_items(self):
        """Test parsing release notes with checked items."""
        body = """
## Description
Some description here.

### Release notes
- [x] Protocol: Added new feature X
- [ ] CLI: Not checked item
- [x] GraphQL: Updated schema
"""
        pr, notes = parse_notes("123", body)

        self.assertEqual(pr, "123")
        self.assertEqual(len(notes), 3)

        self.assertTrue(notes["Protocol"].checked)
        self.assertEqual(notes["Protocol"].note, "Added new feature X")

        self.assertFalse(notes["CLI"].checked)
        self.assertEqual(notes["CLI"].note, "Not checked item")

        self.assertTrue(notes["GraphQL"].checked)
        self.assertEqual(notes["GraphQL"].note, "Updated schema")

    def test_parse_notes_with_multiline_content(self):
        """Test parsing release notes with multiline content."""
        body = """
### Release notes
- [x] Protocol: This is a longer note
  that spans multiple lines
  with more details
- [x] CLI: Single line note
"""
        pr, notes = parse_notes("456", body)

        self.assertEqual(pr, "456")
        self.assertTrue(notes["Protocol"].checked)
        self.assertIn("longer note", notes["Protocol"].note)
        self.assertIn("multiple lines", notes["Protocol"].note)

    def test_parse_notes_no_release_notes_section(self):
        """Test parsing when there's no release notes section."""
        body = """
## Description
Just a regular PR without release notes.
"""
        pr, notes = parse_notes("789", body)

        self.assertEqual(pr, "789")
        self.assertEqual(len(notes), 0)

    def test_parse_notes_empty_body(self):
        """Test parsing empty body."""
        pr, notes = parse_notes("100", "")

        self.assertEqual(pr, "100")
        self.assertEqual(len(notes), 0)

    def test_parse_notes_case_insensitive_heading(self):
        """Test that release notes heading is case insensitive."""
        body = """
### RELEASE NOTES
- [x] Protocol: Test note
"""
        pr, notes = parse_notes("101", body)

        self.assertEqual(len(notes), 1)
        self.assertTrue(notes["Protocol"].checked)

    def test_parse_notes_uppercase_x(self):
        """Test that uppercase X is recognized as checked."""
        body = """
### Release notes
- [X] Protocol: Uppercase X
- [x] CLI: Lowercase x
"""
        pr, notes = parse_notes("102", body)

        self.assertTrue(notes["Protocol"].checked)
        self.assertTrue(notes["CLI"].checked)

    def test_parse_notes_with_space_checkbox(self):
        """Test parsing notes with space checkbox (standard unchecked format)."""
        body = """
### Release notes
- [ ] Protocol: Space checkbox unchecked
- [ ] CLI: Another unchecked
"""
        pr, notes = parse_notes("103", body)

        self.assertFalse(notes["Protocol"].checked)
        self.assertFalse(notes["CLI"].checked)

    def test_parse_notes_preserves_note_content(self):
        """Test that note content is preserved correctly."""
        body = """
### Release notes
- [x] Protocol: Note with `code` and **bold** text
"""
        pr, notes = parse_notes("104", body)

        self.assertIn("`code`", notes["Protocol"].note)
        self.assertIn("**bold**", notes["Protocol"].note)


class TestNoteOrder(unittest.TestCase):
    """Tests for NOTE_ORDER constant."""

    def test_note_order_contains_expected_areas(self):
        """Test that NOTE_ORDER contains expected impact areas."""
        expected = ["Protocol", "JSON-RPC", "GraphQL", "CLI"]
        for area in expected:
            self.assertIn(area, NOTE_ORDER)

    def test_note_order_no_duplicates(self):
        """Test that NOTE_ORDER has no duplicates."""
        self.assertEqual(len(NOTE_ORDER), len(set(NOTE_ORDER)))


class TestInterestingDirectories(unittest.TestCase):
    """Tests for INTERESTING_DIRECTORIES constant."""

    def test_interesting_directories_contains_expected(self):
        """Test that INTERESTING_DIRECTORIES contains expected paths."""
        expected = ["consensus", "crates", "external-crates", "sui-execution"]
        for directory in expected:
            self.assertIn(directory, INTERESTING_DIRECTORIES)

    def test_interesting_directories_no_duplicates(self):
        """Test that INTERESTING_DIRECTORIES has no duplicates."""
        self.assertEqual(
            len(INTERESTING_DIRECTORIES), len(set(INTERESTING_DIRECTORIES))
        )


class TestPrHasReleaseNotes(unittest.TestCase):
    """Tests for pr_has_release_notes function."""

    @patch("release_notes.gh_api")
    def test_pr_has_release_notes_true(self, mock_gh_api):
        """Test that PR with checked release notes returns True."""
        from release_notes import pr_has_release_notes

        mock_gh_api.return_value = {
            "body": "### Release notes\n- [x] Protocol: Some note"
        }

        result = pr_has_release_notes("123")

        self.assertTrue(result)
        mock_gh_api.assert_called_once_with("/repos/MystenLabs/sui/pulls/123")

    @patch("release_notes.gh_api")
    def test_pr_has_release_notes_false_unchecked(self, mock_gh_api):
        """Test that PR with only unchecked release notes returns False."""
        from release_notes import pr_has_release_notes

        mock_gh_api.return_value = {
            "body": "### Release notes\n- [ ] Protocol: Some note"
        }

        result = pr_has_release_notes("123")

        self.assertFalse(result)
        mock_gh_api.assert_called_once_with("/repos/MystenLabs/sui/pulls/123")

    @patch("release_notes.gh_api")
    def test_pr_has_release_notes_false_no_body(self, mock_gh_api):
        """Test that PR with no body returns False."""
        from release_notes import pr_has_release_notes

        mock_gh_api.return_value = {"body": None}

        result = pr_has_release_notes("123")

        self.assertFalse(result)
        mock_gh_api.assert_called_once_with("/repos/MystenLabs/sui/pulls/123")

    @patch("release_notes.gh_api")
    def test_pr_has_release_notes_api_failure(self, mock_gh_api):
        """Test that API failure returns False."""
        from release_notes import pr_has_release_notes

        mock_gh_api.return_value = None

        result = pr_has_release_notes("123")

        self.assertFalse(result)
        mock_gh_api.assert_called_once_with("/repos/MystenLabs/sui/pulls/123")


class TestGetPrForCommit(unittest.TestCase):
    """Tests for get_pr_for_commit function."""

    @patch("release_notes.gh_api")
    def test_get_pr_for_commit_found(self, mock_gh_api):
        """Test getting PR number for a commit."""
        from release_notes import get_pr_for_commit

        mock_gh_api.return_value = [{"number": 12345}]

        result = get_pr_for_commit("abc123def456")

        self.assertEqual(result, 12345)
        mock_gh_api.assert_called_once_with(
            "/repos/MystenLabs/sui/commits/abc123def456/pulls"
        )

    @patch("release_notes.gh_api")
    def test_get_pr_for_commit_not_found(self, mock_gh_api):
        """Test when commit has no associated PR."""
        from release_notes import get_pr_for_commit

        mock_gh_api.return_value = []

        result = get_pr_for_commit("abc123def456")

        self.assertIsNone(result)
        mock_gh_api.assert_called_once_with(
            "/repos/MystenLabs/sui/commits/abc123def456/pulls"
        )

    @patch("release_notes.gh_api")
    def test_get_pr_for_commit_api_failure(self, mock_gh_api):
        """Test API failure returns None."""
        from release_notes import get_pr_for_commit

        mock_gh_api.return_value = None

        result = get_pr_for_commit("abc123def456")

        self.assertIsNone(result)
        mock_gh_api.assert_called_once_with(
            "/repos/MystenLabs/sui/commits/abc123def456/pulls"
        )


class TestDoGetNotes(unittest.TestCase):
    """Tests for do_get_notes function."""

    @patch("release_notes.extract_notes_for_pr")
    def test_do_get_notes_output(self, mock_extract):
        """Test that do_get_notes outputs correctly formatted notes."""
        from release_notes import do_get_notes

        mock_extract.return_value = (
            "123",
            {
                "Protocol": Note(checked=True, note="Protocol change"),
                "CLI": Note(checked=True, note="CLI change"),
                "GraphQL": Note(checked=False, note="Unchecked note"),
            },
        )

        captured = io.StringIO()
        sys.stdout = captured
        try:
            do_get_notes("123")
        finally:
            sys.stdout = sys.__stdout__

        output = captured.getvalue()
        self.assertIn("Protocol: Protocol change", output)
        self.assertIn("CLI: CLI change", output)
        self.assertNotIn("GraphQL", output)  # Unchecked should not appear


class TestExtractNotesForPr(unittest.TestCase):
    """Tests for extract_notes_for_pr function."""

    @patch("release_notes.gh_api")
    def test_extract_notes_for_pr_success(self, mock_gh_api):
        """Test extracting notes from PR body."""
        from release_notes import extract_notes_for_pr

        mock_gh_api.return_value = {
            "body": "### Release notes\n- [x] Protocol: Test note"
        }

        pr, notes = extract_notes_for_pr("123")

        self.assertEqual(pr, "123")
        self.assertEqual(len(notes), 1)
        self.assertTrue(notes["Protocol"].checked)
        self.assertEqual(notes["Protocol"].note, "Test note")
        mock_gh_api.assert_called_once_with("/repos/MystenLabs/sui/pulls/123")

    @patch("release_notes.gh_api")
    def test_extract_notes_for_pr_api_failure(self, mock_gh_api):
        """Test that API failure returns empty notes."""
        from release_notes import extract_notes_for_pr

        mock_gh_api.return_value = None

        pr, notes = extract_notes_for_pr("123")

        self.assertEqual(pr, "123")
        self.assertEqual(len(notes), 0)
        mock_gh_api.assert_called_once_with("/repos/MystenLabs/sui/pulls/123")

    @patch("release_notes.gh_api")
    def test_extract_notes_for_pr_multiple_notes(self, mock_gh_api):
        """Test extracting multiple release notes from PR body."""
        from release_notes import extract_notes_for_pr

        mock_gh_api.return_value = {
            "body": """## Description
Some PR description.

### Release notes
- [x] Protocol: Protocol change description
- [ ] CLI: Unchecked CLI note
- [x] GraphQL: GraphQL schema update
- [x] JSON-RPC: New RPC method added
"""
        }

        pr, notes = extract_notes_for_pr("456")

        self.assertEqual(pr, "456")
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
