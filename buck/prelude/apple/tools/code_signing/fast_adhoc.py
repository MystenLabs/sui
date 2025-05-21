# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import logging
import os
import subprocess
import sys

from pathlib import Path
from typing import Optional

from .apple_platform import ApplePlatform

_LOGGER = logging.getLogger(__name__)


def _find_executable_for_signed_path(path: Path, platform: ApplePlatform) -> Path:
    extension = path.suffix
    if extension not in [".app", ".appex", ".framework"]:
        return path

    contents_subdir = "Contents/MacOS" if platform.is_desktop() else ""
    contents_dir = path / contents_subdir
    # TODO(): Read binary name from Info.plist
    return contents_dir / path.stem


def _logged_subprocess_run(name, why, args):
    _LOGGER.info(f"  Calling {name} to {why}: `{args}`")
    result = subprocess.run(
        args,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        encoding="utf-8",
    )

    _LOGGER.info(f"  {name} exit code: {result.returncode}")
    if result.stdout:
        _LOGGER.info(f"  {name} stdout:")
        _LOGGER.info(
            "\n" + "\n".join([f"    {line}" for line in result.stdout.split("\n")])
        )
    if result.stderr:
        _LOGGER.info(f"  {name} stderr:")
        _LOGGER.info(
            "\n" + "\n".join([f"    {line}" for line in result.stderr.split("\n")])
        )

    return result


def is_fast_adhoc_codesign_allowed() -> bool:
    if sys.platform != "darwin":
        # This is a macOS-only optimisation
        _LOGGER.info(
            f"Running on non-macOS ({sys.platform}), fast adhoc signing not allowed"
        )
        return False
    if not os.path.exists("/var/db/xcode_select_link"):
        _LOGGER.info(
            "Developer tools do not exist, cannot use `otool`, fast adhoc signing not allowed"
        )
        return False

    return True


def should_skip_adhoc_signing_path(
    path: Path,
    identity_fingerprint: str,
    entitlements_path: Optional[Path],
    platform: ApplePlatform,
):
    logging.getLogger(__name__).info(
        f"Checking if should skip adhoc signing path `{path}` with identity `{identity_fingerprint}` and entitlements `{entitlements_path}` for platform `{platform}`"
    )

    if identity_fingerprint != "-":
        # Non-adhoc environments _always_ code sign
        _LOGGER.info("  Requested non-adhoc signing, not adhoc skipping signing")
        return False

    codesign_args = ["/usr/bin/codesign", "-d", "-v", path]
    codesign_result = _logged_subprocess_run(
        "codesign", "check pre-existing signature", codesign_args
    )

    # Anything that's _already_ adhoc signed can be skipped.
    # On ARM64 systems, the linker will already codesign using adhoc,
    # so performing the signing twice is unnecessary.
    #
    # The entitlements file can be ignored under adhoc signing because:
    #
    # - Frameworks/dylibs do not need entitlements (they operate under the entitlements of their loading binary)
    # - Apps (+ app extensions) have binaries which embed the entitlements via __entitlements section at link time
    #
    # Note that certain features require non-adhoc signing (e.g., app groups) while other features like keychain
    # and "Sign in with Apple" just need the entitlements present in the binary (which it will per the above).
    is_adhoc_signed = "Signature=adhoc" in codesign_result.stderr
    if not is_adhoc_signed:
        _LOGGER.info("  Path is not adhoc signed, not skipping adhoc signing")
        return False

    if entitlements_path:
        # Adhoc entitlements do not require postprocessing, so we just need to check existence
        binary_path = _find_executable_for_signed_path(path, platform)
        otool_arg = ["/usr/bin/otool", "-s", "__TEXT", "__entitlements", binary_path]
        otool_result = _logged_subprocess_run(
            "otool", "check entitlements presence in binary", otool_arg
        )

        contains_entitlements = (
            "Contents of (__TEXT,__entitlements) section" in otool_result.stdout
        )
        if not contains_entitlements:
            _LOGGER.info(
                f"  Binary path `{binary_path}` does not contain entitlements, not skipping adhoc signing"
            )
            return False

    _LOGGER.info(f"  All checks passed for `{path}`, skipping adhoc signing")
    return True
