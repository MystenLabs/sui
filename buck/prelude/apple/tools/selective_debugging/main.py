#!/usr/bin/env fbpython
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
import sys

from .scrubber import scrub


def _parse_args():
    parser = argparse.ArgumentParser(
        description="Tool to postprocess executables/dylibs."
    )
    parser.add_argument(
        "--input",
        required=True,
        help="Path to the input which is an executable/dylib file.",
    )
    parser.add_argument("--output", required=True, help="Path to the output file.")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument(
        "--targets-file",
        help="Path to a json file which contains user-focused Buck targets",
    )
    group.add_argument(
        "--spec-file",
        help="Path to a json file which contains user-focused include/exclude specs",
    )
    parser.add_argument(
        "--adhoc-codesign-tool",
        help="An adhoc codesign tool to use to re-sign the executables/dylibs, if provided.",
    )
    return parser.parse_args()


def main():
    args = _parse_args()
    try:
        scrub(
            input_file=args.input,
            output_file=args.output,
            targets_file=args.targets_file,
            spec_file=args.spec_file,
            adhoc_codesign_tool=args.adhoc_codesign_tool,
        )
    except Exception as e:
        print(f"Focused debugging failed: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
