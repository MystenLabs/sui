# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import json
import re
from dataclasses import dataclass, field
from typing import List


@dataclass
class BuildTargetPatternOutputPathMatcher:
    pattern: str
    output_path: str = field(init=False)

    def __post_init__(self) -> None:
        _, package_and_name = self.pattern.split("//")
        if package_and_name.endswith("..."):
            # recursive pattern
            output_path, _ = package_and_name.split("...")
        elif package_and_name.endswith(":"):
            # package pattern
            package, _ = package_and_name.split(":")
            # This assumes the output path created by buck2, which if
            # modified, would break this logic.
            output_path = f"{package}/__"
        else:
            # target pattern
            package, name = package_and_name.split(":")
            # This assumes the output path created by buck2, which if
            # modified, would break this logic.
            output_path = f"{package}/__{name}__"

        self.output_path = output_path

    def match_path(self, debug_file_path: str) -> bool:
        return self.output_path in debug_file_path


@dataclass
class Spec:
    spec_path: str
    include_build_target_patterns: List[BuildTargetPatternOutputPathMatcher] = field(
        init=False
    )
    include_regular_expressions: List[re.Pattern] = field(init=False)
    exclude_build_target_patterns: List[BuildTargetPatternOutputPathMatcher] = field(
        init=False
    )
    exclude_regular_expressions: List[re.Pattern] = field(init=False)

    def __post_init__(self) -> None:
        with open(self.spec_path, "r") as f:
            data = json.load(f)

        self.include_build_target_patterns = [
            BuildTargetPatternOutputPathMatcher(entry)
            for entry in data["include_build_target_patterns"]
        ]
        self.include_regular_expressions = [
            re.compile(entry) for entry in data["include_regular_expressions"]
        ]
        self.exclude_build_target_patterns = [
            BuildTargetPatternOutputPathMatcher(entry)
            for entry in data["exclude_build_target_patterns"]
        ]
        self.exclude_regular_expressions = [
            re.compile(entry) for entry in data["exclude_regular_expressions"]
        ]

    def scrub_debug_file_path(self, debug_file_path: str) -> bool:
        if self.include_build_target_patterns or self.include_regular_expressions:
            is_included = _path_matches_pattern_or_expression(
                debug_file_path,
                self.include_build_target_patterns,
                self.include_regular_expressions,
            )
        else:
            is_included = True

        # If the path is included (and not excluded), do not scrub
        return not (
            is_included
            and not _path_matches_pattern_or_expression(
                debug_file_path,
                self.exclude_build_target_patterns,
                self.exclude_regular_expressions,
            )
        )


def _path_matches_pattern_or_expression(
    debug_file_path: str,
    patterns: List[BuildTargetPatternOutputPathMatcher],
    expressions: List[re.Pattern],
) -> bool:
    for pattern in patterns:
        if pattern.match_path(debug_file_path):
            return True
    for expression in expressions:
        if expression.search(debug_file_path):
            return True
    return False
