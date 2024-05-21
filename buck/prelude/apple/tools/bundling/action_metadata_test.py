# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import unittest
from json import JSONDecodeError
from pathlib import Path

import pkg_resources

from .action_metadata import parse_action_metadata


class TestActionMetadata(unittest.TestCase):
    def test_valid_metadata_is_parsed_successfully(self):
        file_content = pkg_resources.resource_stream(
            __name__, "test_resources/valid_action_metadata.json"
        )
        result = parse_action_metadata(file_content)
        self.assertEqual(
            result,
            {
                Path("repo/foo.txt"): "foo_digest",
                Path("buck-out/bar.txt"): "bar_digest",
            },
        )

    def test_error_when_invalid_metadata(self):
        file_content = pkg_resources.resource_stream(
            __name__, "test_resources/the.broken_json"
        )
        with self.assertRaises(JSONDecodeError):
            _ = parse_action_metadata(file_content)

    def test_user_friendly_error_when_metadata_with_newer_version(self):
        file_content = pkg_resources.resource_stream(
            __name__, "test_resources/newer_version_action_metadata.json"
        )
        with self.assertRaises(Exception) as context:
            _ = parse_action_metadata(file_content)
            self.assertEqual(
                context.exception,
                RuntimeError("Expected metadata version to be `1` got `2`."),
            )
