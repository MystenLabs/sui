#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
import os
import shutil


def extract_file(package, src, dst):
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copyfile(os.path.join(package, src), dst)


def extract_directory(package, src, dst):
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copytree(os.path.join(package, src), dst)


def main():
    parser = argparse.ArgumentParser(
        prog="conan_package_extract",
        description="Extract outputs from a Conan package.",
    )
    parser.add_argument(
        "--package",
        metavar="PATH",
        type=str,
        required=True,
        help="Path to the package output directory.",
    )
    parser.add_argument(
        "--file-from",
        metavar="PATH",
        type=str,
        required=False,
        action="append",
        default=[],
        help="File to extract. All --file-* arguments must align.",
    )
    parser.add_argument(
        "--file-to",
        metavar="PATH",
        type=str,
        required=False,
        action="append",
        default=[],
        help="Destination to extract the file to. All --file-* arguments must align.",
    )
    parser.add_argument(
        "--directory-from",
        metavar="PATH",
        type=str,
        required=False,
        action="append",
        default=[],
        help="Directory to extract. All --directory-* arguments must align.",
    )
    parser.add_argument(
        "--directory-to",
        metavar="PATH",
        type=str,
        required=False,
        action="append",
        default=[],
        help="Destination to extract the directory to. All --directory-* arguments must align.",
    )
    args = parser.parse_args()

    assert len(args.file_from) == len(args.file_to), "Mismatching file arguments."
    assert len(args.directory_from) == len(
        args.directory_to
    ), "Mismatching directory arguments."
    for src, dst in zip(args.file_from, args.file_to):
        extract_file(args.package, src, dst)
    for src, dst in zip(args.directory_from, args.directory_to):
        extract_directory(args.package, src, dst)


if __name__ == "__main__":
    main()
