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

from .incremental_state import (
    IncrementalState,
    IncrementalStateItem,
    parse_incremental_state,
)


class TestIncrementalState(unittest.TestCase):
    def test_valid_state_is_parsed_successfully(self):
        file_content = pkg_resources.resource_stream(
            __name__, "test_resources/valid_incremental_state.json"
        )
        result = parse_incremental_state(file_content)
        expected = IncrementalState(
            items=[
                IncrementalStateItem(
                    source=Path("repo/foo.txt"),
                    destination_relative_to_bundle=Path("foo.txt"),
                    digest="foo_digest",
                    resolved_symlink=None,
                ),
                IncrementalStateItem(
                    source=Path("buck-out/bar.txt"),
                    destination_relative_to_bundle=Path("Resources/bar.txt"),
                    digest="bar_digest",
                    resolved_symlink=None,
                ),
                IncrementalStateItem(
                    source=Path("buck-out/bar"),
                    destination_relative_to_bundle=Path("Resources/bar"),
                    digest=None,
                    resolved_symlink=Path("bar.txt"),
                ),
            ],
            codesigned=True,
            codesign_configuration=None,
            codesign_on_copy_paths=[Path("Resources/bar.txt")],
            codesign_identity="Johny Appleseed",
            swift_stdlib_paths=[Path("Frameworks/libswiftCore.dylib")],
        )
        self.assertEqual(
            result,
            expected,
        )

    def test_error_when_invalid_metadata(self):
        file_content = pkg_resources.resource_stream(
            __name__, "test_resources/the.broken_json"
        )
        with self.assertRaises(JSONDecodeError):
            _ = parse_incremental_state(file_content)

    def test_user_friendly_error_when_metadata_with_newer_version(self):
        file_content = pkg_resources.resource_stream(
            __name__, "test_resources/newer_version_incremental_state.json"
        )
        with self.assertRaises(Exception) as context:
            _ = parse_incremental_state(file_content)
            self.assertEqual(
                context.exception,
                RuntimeError("Expected incremental state version to be `2` got `3`."),
            )
