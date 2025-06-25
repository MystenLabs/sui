# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.


import argparse
import os
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Filter the ABIs in a prebuilt native library directory"
    )
    parser.add_argument(
        "library_dirs",
        type=Path,
        help="A file that lists directories containing ABI subdirectories",
    )
    parser.add_argument(
        "output_dir",
        type=Path,
        help="Symlinks to the filtered ABIs will be created here",
    )
    parser.add_argument(
        "--abis",
        required=True,
        nargs="+",
        help="The ABIs to filter",
    )
    args = parser.parse_args()

    args.output_dir.mkdir(parents=True)
    with open(args.library_dirs, "r") as f:
        for line in f:
            library_dir = Path(line.rstrip())
            for abi in args.abis:
                abi_path = library_dir / abi
                if abi_path.exists():
                    abi_output_path = args.output_dir / abi
                    abi_output_path.mkdir(exist_ok=True)
                    for source_path in abi_path.iterdir():
                        # This should be a directory, but files will be silently ignored by APK
                        # packaging, so we don't need to error here. We use os.path.relpath because
                        # pathlib.Path.relative_to only works if the paths have a common prefix.
                        (abi_output_path / source_path.name).symlink_to(
                            os.path.relpath(source_path, start=abi_output_path),
                            target_is_directory=source_path.is_dir(),
                        )


if __name__ == "__main__":
    main()
