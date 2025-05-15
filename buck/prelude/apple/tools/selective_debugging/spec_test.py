# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import json
import unittest
from tempfile import NamedTemporaryFile
from typing import Dict, List

from .spec import BuildTargetPatternOutputPathMatcher, Spec

FAKE_PATH = "fake/path"


class Test(unittest.TestCase):
    def test_build_target_pattern_matcher(self):
        target = BuildTargetPatternOutputPathMatcher("cell//foo:bar")
        self.assertTrue(target.match_path("foo/__bar__"))
        self.assertFalse(target.match_path("foo/__baz__"))

        package = BuildTargetPatternOutputPathMatcher("cell//foo:")
        self.assertTrue(package.match_path("foo/__bar__"))
        self.assertTrue(package.match_path("foo/__baz__"))
        self.assertFalse(target.match_path("foo/bar/__baz__"))

        recursive = BuildTargetPatternOutputPathMatcher("cell//foo/...")
        self.assertTrue(recursive.match_path("foo/__bar__"))
        self.assertTrue(recursive.match_path("foo/bar/__baz__"))
        self.assertFalse(recursive.match_path("bar/__baz__"))

    def test_spec_with_includes(self):
        test_spec = _base_spec()
        test_spec["include_build_target_patterns"] = [
            "cell//foo:",
        ]

        spec = _get_spec(test_spec)

        # We expect to not scrub anything with "foo/__"
        self.assertFalse(spec.scrub_debug_file_path("foo/__bar__"))
        self.assertFalse(spec.scrub_debug_file_path("foo/__baz__"))
        self.assertTrue(spec.scrub_debug_file_path("foo/bar/__baz__"))

    def test_spec_with_include_regex(self):
        test_spec = _base_spec()
        test_spec["include_regular_expressions"] = [
            "foo",
        ]

        spec = _get_spec(test_spec)

        # We expect to not scrub anything with "foo"
        self.assertFalse(spec.scrub_debug_file_path("foo/__bar__"))
        self.assertFalse(spec.scrub_debug_file_path("foo/__baz__"))
        self.assertTrue(spec.scrub_debug_file_path("bar/bar/__baz__"))

    def test_spec_with_exclude_regex(self):
        test_spec = _base_spec()
        test_spec["exclude_regular_expressions"] = [
            "foo",
        ]

        spec = _get_spec(test_spec)

        # We expect to scrub anything with "foo"
        self.assertTrue(spec.scrub_debug_file_path("foo/__bar__"))
        self.assertTrue(spec.scrub_debug_file_path("foo/__baz__"))
        self.assertFalse(spec.scrub_debug_file_path("bar/bar/__baz__"))

    def test_spec_with_both(self):
        test_spec = _base_spec()
        test_spec["include_build_target_patterns"] = [
            "cell//foo:",
        ]
        test_spec["exclude_regular_expressions"] = [
            "bar",
        ]

        spec = _get_spec(test_spec)

        # We expect to scrub anything with "bar", and not scrub anything with "foo/__"
        self.assertTrue(spec.scrub_debug_file_path("foo/__bar__"))
        self.assertFalse(spec.scrub_debug_file_path("foo/__baz__"))
        self.assertTrue(spec.scrub_debug_file_path("bar/bar/__baz__"))


def _get_spec(test_spec) -> Spec:
    with NamedTemporaryFile(mode="w+") as tmp:
        json.dump(test_spec, tmp)
        tmp.flush()

        return Spec(tmp.name)


def _base_spec() -> Dict[str, List[str]]:
    return {
        "include_build_target_patterns": [],
        "include_regular_expressions": [],
        "exclude_build_target_patterns": [],
        "exclude_regular_expressions": [],
    }
