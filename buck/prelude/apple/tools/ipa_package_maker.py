# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
import os
import shutil
import subprocess

import tempfile

from pathlib import Path

from typing import Optional

from apple.tools.re_compatibility_utils.writable import make_dir_recursively_writable


def _copy_ipa_contents(ipa_contents_dir: Path, output_dir: Path):
    if os.path.exists(output_dir):
        shutil.rmtree(output_dir, ignore_errors=False)
    shutil.copytree(ipa_contents_dir, output_dir, symlinks=True, dirs_exist_ok=False)


def _delete_empty_SwiftSupport_dir(output_dir: Path):
    swiftSupportDir = output_dir / "SwiftSupport"
    if not swiftSupportDir.exists():
        return

    swiftSupportDirHasFiles = False
    for _, _, files in os.walk(swiftSupportDir):
        if files:
            swiftSupportDirHasFiles = True
            break

    if not swiftSupportDirHasFiles:
        shutil.rmtree(swiftSupportDir)


def _package_ipa_contents(
    ipa_contents_dir: Path,
    ipa_output_path: Path,
    compression_level: int,
    validator: Optional[Path],
):
    with tempfile.TemporaryDirectory() as processed_package_dir:
        processed_package_dir_path = Path(processed_package_dir)
        _copy_ipa_contents(ipa_contents_dir, processed_package_dir_path)

        # Apple requires SwiftSupport dir to be non-empty
        _delete_empty_SwiftSupport_dir(processed_package_dir_path)

        # Apple requires all executable files in an `.ipa` to be _writable_, otherwise App Store validation fails with:
        #   "Asset validation failed (90711) Invalid executable permissions. The executable $X does not have its writable bit set."
        # Furthermore, there's additional internal infra that needs certain files to be writable.
        #
        # In normal development outside Meta, all files in an .ipa will be user writable, so let's just the sensible thing
        # and mirror behavior which Apple expects, so we're future-proof.
        make_dir_recursively_writable(str(processed_package_dir_path))

        if validator:
            validation_command = [
                str(validator),
                "--ipa-contents-dir",
                str(processed_package_dir_path),
            ]
            subprocess.run(
                validation_command,
                check=True,
            )

        with open(ipa_output_path, "wb") as ipa_file:
            zip_cmd = ["zip", "-X", "-r", f"-{compression_level}", "-", "."]
            subprocess.run(
                zip_cmd,
                # .ipa zip file requires to be created relative to the package dir,
                # zip command has no way to express apart from changing cwd
                cwd=processed_package_dir_path,
                stdout=ipa_file,
                check=True,
            )


def main():
    parser = argparse.ArgumentParser(description="Tool to make an .ipa package file.")
    parser.add_argument(
        "--ipa-contents-dir",
        required=True,
        type=Path,
        help="The directory which has the contents of the .ipa file.",
    )
    parser.add_argument(
        "--ipa-output-path",
        required=True,
        type=Path,
        help="The path to the output .ipa file.",
    )
    parser.add_argument(
        "--compression-level",
        type=int,
        required=True,
        help="The compression level to use for 'zip'.",
    )
    parser.add_argument(
        "--validator",
        type=Path,
        required=False,
        help="A path to an executable which will be passed the path to the IPA contents dir to validate",
    )

    args = parser.parse_args()
    _package_ipa_contents(
        args.ipa_contents_dir,
        args.ipa_output_path,
        args.compression_level,
        args.validator,
    )


if __name__ == "__main__":
    main()
