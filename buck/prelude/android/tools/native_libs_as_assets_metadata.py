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
from typing import NamedTuple


class NativeLibrary(NamedTuple):
    full_path: Path
    relative_path: Path
    size: int
    sha256: str


def _get_native_library(path: Path, relative_path: Path) -> NativeLibrary:
    if not (path.name == "wrap.sh" or path.suffix == ".so"):
        raise Exception("Unexpected path {} in native library directory!".format(path))

    with open(path, "rb") as f:
        file_size = os.path.getsize(path)
        sha256 = hashlib.sha256(f.read()).hexdigest()

        return NativeLibrary(path, relative_path, file_size, sha256)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Writes out metadata relating to native libraries"
    )
    parser.add_argument(
        "--native-library-dirs",
        type=Path,
        help="A file of directories containing native libraries",
    )
    parser.add_argument(
        "--metadata-output",
        type=Path,
        help="Metadata is written to this file",
    )
    parser.add_argument(
        "--native-library-paths-output",
        type=Path,
        help="The actual paths of all the native libraries",
    )
    args = parser.parse_args()

    native_libraries = []
    with open(args.native_library_dirs) as f:
        for line in f:
            native_library_dir = Path(line.strip())
            for full_path in native_library_dir.rglob("*"):
                if full_path.is_file():
                    native_libraries.append(
                        _get_native_library(
                            full_path, full_path.relative_to(native_library_dir)
                        )
                    )

    # buck1 sorts native libraries in decreasing file size order, so we do the same.
    native_libraries.sort(
        key=lambda native_lib: (-native_lib.size, native_lib.relative_path)
    )

    with open(args.metadata_output, "w") as f:
        f.write(
            "\n".join(
                [
                    "{} {} {}".format(
                        str(native_lib.relative_path),
                        str(native_lib.size),
                        str(native_lib.sha256),
                    )
                    for native_lib in native_libraries
                ]
            )
        )

    with open(args.native_library_paths_output, "w") as f:
        f.write(
            "\n".join([str(native_lib.full_path) for native_lib in native_libraries])
        )


if __name__ == "__main__":
    main()
