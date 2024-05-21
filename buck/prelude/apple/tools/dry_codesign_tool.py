# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
import plistlib
import shutil

from pathlib import Path

_CODE_SIGN_DRY_RUN_ARGS_FILE = "BUCK_code_sign_args.plist"
_CODE_SIGN_DRY_RUN_ENTITLEMENTS_FILE = "BUCK_code_sign_entitlements.plist"


def _args_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="""
            Tool which implements `DryCodeSignStep` class from buck1.
            Instead of code signing the bundle it just creates a file named `BUCK_code_sign_args.plist` inside,
             which contains all parameters needed to perform a deferred signing later.
        """
    )
    parser.add_argument(
        "root",
        type=Path,
    )
    parser.add_argument(
        "--entitlements",
        metavar="<Entitlements.plist>",
        type=Path,
        required=False,
        help="Path to file with entitlements to be used during code signing.",
    )
    parser.add_argument(
        "--identity",
        type=str,
        required=True,
    )
    parser.add_argument(
        "--extra-paths-to-sign",
        type=str,
        nargs="*",
    )

    return parser


def _main():
    args = _args_parser().parse_args()
    content = {
        # This is always empty string if you check `DryCodeSignStep` class usages in buck1
        "relative-path-to-sign": "",
        "use-entitlements": args.entitlements is not None,
        "identity": args.identity,
    }
    if args.extra_paths_to_sign:
        content["extra-paths-to-sign"] = args.extra_paths_to_sign
    with open(args.root / _CODE_SIGN_DRY_RUN_ARGS_FILE, "wb") as f:
        # Do not sort to keep the ordering same as in buck1.
        plistlib.dump(content, f, sort_keys=False, fmt=plistlib.FMT_XML)
    if args.entitlements:
        shutil.copy2(
            args.entitlements,
            args.root / _CODE_SIGN_DRY_RUN_ENTITLEMENTS_FILE,
        )


if __name__ == "__main__":
    _main()
