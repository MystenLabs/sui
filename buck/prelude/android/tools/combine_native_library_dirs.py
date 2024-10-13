# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.


import argparse
import hashlib
import os
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Combine the native libraries in different directories into a single symlink"
    )
    parser.add_argument(
        "--library-dirs",
        type=Path,
        nargs="+",
        help="Paths to the dirs that should be combined",
        required=True,
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        required=True,
    )
    parser.add_argument(
        "--metadata-file",
        type=Path,
        required=False,
    )
    args = parser.parse_args()

    metadata_lines = []

    args.output_dir.mkdir(parents=True)
    for library_dir in args.library_dirs:
        all_libs = library_dir.glob("**/*.s[o|h]")
        for lib in all_libs:
            relative_path = lib.relative_to(library_dir)
            output_path = args.output_dir / relative_path
            output_path.parent.mkdir(exist_ok=True)
            output_path.symlink_to(os.readlink(lib))

            if args.metadata_file:
                with open(lib, "rb") as f:
                    metadata_lines.append(
                        "{} {}".format(
                            relative_path, hashlib.sha1(f.read()).hexdigest()
                        )
                    )

    if args.metadata_file:
        with open(args.metadata_file, "w") as f:
            f.write("\n".join(metadata_lines))


if __name__ == "__main__":
    main()
