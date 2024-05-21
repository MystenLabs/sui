# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
import pathlib
import sys

from .apple_platform import ApplePlatform
from .codesign_bundle import (
    AdhocSigningContext,
    codesign_bundle,
    non_adhoc_signing_context,
)
from .provisioning_profile_selection import CodeSignProvisioningError


def _args_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Tool which code signs the Apple bundle. `Info.plist` file is amended as a part of it."
    )
    parser.add_argument(
        "--bundle-path",
        metavar="</path/to/app.bundle>",
        type=pathlib.Path,
        required=True,
        help="Absolute path to Apple bundle result.",
    )
    parser.add_argument(
        "--info-plist",
        metavar="<Info.plist>",
        type=pathlib.Path,
        required=True,
        help="Bundle relative destination path to Info.plist file if it is present in bundle.",
    )
    parser.add_argument(
        "--entitlements",
        metavar="<Entitlements.plist>",
        type=pathlib.Path,
        required=False,
        help="Path to file with entitlements to be used during code signing. If it's not provided the minimal entitlements are going to be generated.",
    )
    parser.add_argument(
        "--profiles-dir",
        metavar="</provisioning/profiles/directory>",
        type=pathlib.Path,
        required=False,
        help="Path to directory with provisioning profile files. Required if code signing is not ad-hoc.",
    )
    parser.add_argument(
        "--ad-hoc",
        action="store_true",
        help="Perform ad-hoc signing if set.",
    )
    parser.add_argument(
        "--ad-hoc-codesign-identity",
        metavar="<identity>",
        type=str,
        required=False,
        help="Codesign identity to use when ad-hoc signing is performed.",
    )
    parser.add_argument(
        "--platform",
        metavar="<apple platform>",
        type=ApplePlatform,
        required=True,
        help="Apple platform for which the bundle was built.",
    )
    parser.add_argument(
        "--codesign-on-copy",
        metavar="<codesign/this/path>",
        type=pathlib.Path,
        action="append",
        required=False,
        help="Bundle relative path that should be codesigned prior to result bundle.",
    )

    return parser


# Add emoji to beginning of actionable error message so it stands out more.
def decorate_error_message(message: str) -> str:
    return " ".join(["❗️", message])


def _main():
    args = _args_parser().parse_args()
    try:
        if args.ad_hoc:
            signing_context = AdhocSigningContext(
                codesign_identity=args.ad_hoc_codesign_identity
            )
        else:
            assert (
                args.profiles_dir
            ), "Path to directory with provisioning profile files should be set when signing is not ad-hoc."
            signing_context = non_adhoc_signing_context(
                info_plist_source=args.bundle_path / args.info_plist,
                info_plist_destination=args.info_plist,
                provisioning_profiles_dir=args.profiles_dir,
                entitlements_path=args.entitlements,
                platform=args.platform,
            )
        codesign_bundle(
            bundle_path=args.bundle_path,
            signing_context=signing_context,
            entitlements_path=args.entitlements,
            platform=args.platform,
            codesign_on_copy_paths=args.codesign_on_copy or [],
        )
    except CodeSignProvisioningError as e:
        print(decorate_error_message(str(e)), file=sys.stderr)
        exit(1)


if __name__ == "__main__":
    _main()
