# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

from __future__ import annotations

from dataclasses import dataclass
from typing import IO, Optional

from apple.tools.plistlib_utils import detect_format_and_load


@dataclass
class InfoPlistMetadata:
    bundle_id: str
    bundle_type: Optional[str]
    is_watchos_app: bool

    @staticmethod
    def from_file(info_plist_file: IO) -> InfoPlistMetadata:
        root = detect_format_and_load(info_plist_file)
        return InfoPlistMetadata(
            root["CFBundleIdentifier"],
            root.get("CFBundlePackageType"),
            root.get("WKWatchKitApp", False),
        )
