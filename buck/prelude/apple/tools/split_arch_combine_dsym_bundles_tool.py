# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
import os
import shutil

from pathlib import Path


def _args_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="""
            Tool for combining multiple dsym bundles into a single one for Universal Binaries when the
            split_arch_dsym is enabled.
        """
    )
    parser.add_argument("--dsym-bundle", action="append", type=str)
    parser.add_argument("--arch", action="append", type=str)
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Path where the output bundle should be written to",
    )

    return parser


def _main():
    args = _args_parser().parse_args()

    output_dwarf_path = os.path.join(args.output, "Contents/Resources/DWARF")
    os.makedirs(output_dwarf_path)

    if len(args.arch) != len(args.dsym_bundle):
        raise Exception(
            f"Need to specify an architecture for every dsym bundle, archs:{args.arch}, dsyms:{args.dsym_bundle}"
        )
    for i in range(len(args.dsym_bundle)):
        dwarf_files_dir = os.path.join(args.dsym_bundle[i], "Contents/Resources/DWARF")
        dwarf_files = os.listdir(dwarf_files_dir)
        for dwarf_file in dwarf_files:
            shutil.copy2(
                os.path.join(dwarf_files_dir, dwarf_file),
                os.path.join(output_dwarf_path, f"{dwarf_file}.{args.arch[i]}"),
            )

    # pick one of the Info.plist and copy it to the output bundle.
    shutil.copy2(
        os.path.join(args.dsym_bundle[0], "Contents/Info.plist"),
        os.path.join(args.output, "Contents/Info.plist"),
    )


if __name__ == "__main__":
    _main()
