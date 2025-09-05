# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
import pathlib
import shutil

from java.tools import utils

ZIP_NOTHING_TO_DO_EXIT_CODE = 12


def _parse_args():
    parser = argparse.ArgumentParser(
        description="Tool to remove extra resources from apk."
    )
    parser.add_argument(
        "--input-apk",
        type=pathlib.Path,
        required=True,
        help="a path to the original apk",
    )
    parser.add_argument(
        "--output-apk",
        type=pathlib.Path,
        required=True,
        help="a path to the output apk with removed resources",
    )
    parser.add_argument(
        "--extra-filtered-resources",
        type=str,
        action="append",
        required=True,
        help="list of patterns of files to filter out from the input apk",
    )
    return parser.parse_args()


def main():
    args = _parse_args()
    shutil.copyfile(args.input_apk, args.output_apk)
    utils.execute_command(["chmod", "644", args.output_apk])

    # The normal resource filtering apparatus is super slow because it extracts the whole apk,
    # strips files out of it, then repackages it.
    #
    # This is a faster filtering step that just uses zip -d to remove entries from the archive.
    # It's also superbly dangerous.
    #
    # If zip -d returns that there was nothing to do, then we don't fail.
    utils.execute_command_ignore_exit_codes(
        ["zip", "-d", str(args.output_apk)] + args.extra_filtered_resources,
        [ZIP_NOTHING_TO_DO_EXIT_CODE],
    )


if __name__ == "__main__":
    main()
