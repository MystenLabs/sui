# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import plistlib
import unittest

import pkg_resources

from .app_id import AppId


class TestAppId(unittest.TestCase):
    def test_string_parsing(self):
        result = AppId.from_string("ABCDE12345.com.example.TestApp")
        expected = AppId("ABCDE12345", "com.example.TestApp")
        self.assertEqual(expected, result)

        result = AppId.from_string("ABCDE12345.*")
        expected = AppId("ABCDE12345", "*")
        self.assertEqual(expected, result)

        with self.assertRaisesRegex(RuntimeError, "Malformed app ID string: invalid."):
            _ = AppId.from_string("invalid.")

    def test_entitlements_parsing(self):
        file = pkg_resources.resource_stream(
            __name__, "test_resources/Entitlements.plist"
        )
        entitlements = plistlib.load(file)
        result = AppId.infer_from_entitlements(entitlements)
        expected = AppId("ABCDE12345", "com.example.TestApp")
        self.assertEqual(expected, result)
