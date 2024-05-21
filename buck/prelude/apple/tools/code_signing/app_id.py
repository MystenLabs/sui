# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

from __future__ import annotations

import re
from dataclasses import dataclass
from enum import Enum
from typing import Any, Dict, Optional


@dataclass
class AppId:
    team_id: str
    bundle_id: str

    class _ReGroupName(str, Enum):
        team_id = "team_id"
        bundle_id = "bundle_id"

    _re_string = "^(?P<{team_id}>[A-Z0-9]{{10}})\\.(?P<{bundle_id}>.+)$".format(
        team_id=_ReGroupName.team_id,
        bundle_id=_ReGroupName.bundle_id,
    )
    _re_pattern = re.compile(_re_string)

    # Takes a application identifier and splits it into Team ID and bundle ID.
    # Prefix is always a ten-character alphanumeric sequence. Bundle ID may be a fully-qualified name or a wildcard ending in *.
    @classmethod
    def from_string(cls, string: str) -> AppId:
        match = re.match(cls._re_pattern, string)
        if not match:
            raise RuntimeError("Malformed app ID string: {}".format(string))
        return AppId(
            match.group(cls._ReGroupName.team_id),
            match.group(cls._ReGroupName.bundle_id),
        )

    # Returns the App ID if it can be inferred from keys in the entitlement. Otherwise, it returns `None`.
    @staticmethod
    def infer_from_entitlements(entitlements: Dict[str, Any]) -> Optional[AppId]:
        keychain_access_groups = entitlements.get("keychain-access-groups")
        if not keychain_access_groups:
            return None
        app_id_string = keychain_access_groups[0]
        return AppId.from_string(app_id_string)
