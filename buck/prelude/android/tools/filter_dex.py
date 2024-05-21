# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.


import argparse
import json
import pathlib
import re

PREFIX_MARKER = "^"
SUFFIX_MARKER = "^"
REGEX_MARKER = "^-"


class ClassNameFilter:
    def __init__(self, primary_dex_patterns):
        prefixes = []
        suffixes = []
        substrings = []
        exact_matches = []
        regular_expressions = []

        for pattern in primary_dex_patterns:
            if pattern.startswith(REGEX_MARKER):
                regular_expressions.append(pattern[2:])
            else:
                is_prefix = pattern[0] == PREFIX_MARKER
                is_suffix = pattern[-1] == SUFFIX_MARKER
                if is_prefix and is_suffix:
                    exact_matches.append(pattern[1:-1])
                elif is_prefix:
                    prefixes.append(pattern[1:])
                elif is_suffix:
                    suffixes.append(pattern[:-1])
                else:
                    substrings.append(pattern)

        self.prefixes = prefixes
        self.suffixes = suffixes
        self.substrings = substrings
        self.exact_matches = exact_matches
        self.regular_expressions = [
            re.compile(regular_expression) for regular_expression in regular_expressions
        ]

    def class_name_matches_filter(self, class_name):
        if class_name in self.exact_matches:
            return True

        for prefix in self.prefixes:
            if class_name.startswith(prefix):
                return True

        for suffix in self.suffixes:
            if class_name.endswith(suffix):
                return True

        for substring in self.substrings:
            if substring in class_name:
                return True

        for regular_expression in self.regular_expressions:
            if regular_expression.match(class_name):
                return True

        return False


def _parse_args():
    parser = argparse.ArgumentParser(
        description="Tool to filter a dex for primary class names."
    )

    parser.add_argument(
        "--primary-dex-patterns",
        type=pathlib.Path,
        required=True,
        help="a path to a list of primary dex patterns",
    )
    parser.add_argument(
        "--dex-target-identifiers",
        type=str,
        required=True,
        nargs="+",
        help="a list of dex target identifiers",
    )
    parser.add_argument(
        "--class-names",
        type=pathlib.Path,
        required=True,
        nargs="+",
        help="a path to a list of class names",
    )
    parser.add_argument(
        "--weight-estimates",
        type=pathlib.Path,
        required=True,
        nargs="+",
        help="a path to a weight estimate",
    )
    parser.add_argument(
        "--output",
        type=pathlib.Path,
        required=True,
        help="a path to an output. The output is a JSON mapping of dex target names to a map of primary dex classes, secondary dex classes, and weight estimate.",
    )

    return parser.parse_args()


def main():
    args = _parse_args()

    primary_dex_patterns_path = args.primary_dex_patterns
    with open(primary_dex_patterns_path) as primary_dex_patterns_file:
        all_primary_dex_patterns = [line.rstrip() for line in primary_dex_patterns_file]

    class_name_filter = ClassNameFilter(all_primary_dex_patterns)

    dex_target_identifiers = args.dex_target_identifiers
    class_names_paths = args.class_names
    weight_estimate_paths = args.weight_estimates
    output = args.output

    assert len(dex_target_identifiers) == len(
        class_names_paths
    ), "Must provide same number of class names files as dex target identifiers!"

    assert len(dex_target_identifiers) == len(
        weight_estimate_paths
    ), "Must provide same number of weight estimate files as dex target identifiers!"

    json_output = {}
    for i in range(len(dex_target_identifiers)):
        dex_target_name = dex_target_identifiers[i]
        weight_estimate_path = weight_estimate_paths[i]
        with open(weight_estimate_path) as weight_estimate_file:
            weight_estimate = weight_estimate_file.read().strip()

        class_names_path = class_names_paths[i]
        with open(class_names_path) as class_names_file:
            all_class_names = [line.rstrip() for line in class_names_file]

        primary_dex_class_names = []
        secondary_dex_class_names = []
        for java_class in all_class_names:
            if class_name_filter.class_name_matches_filter(java_class):
                primary_dex_class_names.append(java_class + ".class")
            else:
                secondary_dex_class_names.append(java_class + ".class")

        json_output[dex_target_name] = {
            "primary_dex_class_names": primary_dex_class_names,
            "secondary_dex_class_names": secondary_dex_class_names,
            "weight_estimate": weight_estimate,
        }

    with open(output, "w") as output_file:
        json.dump(json_output, output_file, indent=4)


if __name__ == "__main__":
    main()
