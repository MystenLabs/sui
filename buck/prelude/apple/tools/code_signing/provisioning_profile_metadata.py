# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any, Dict, Set

from apple.tools.plistlib_utils import detect_format_and_loads

from .app_id import AppId


@dataclass
class ProvisioningProfileMetadata:
    # Path to the provisioning profile file
    file_path: Path
    uuid: str
    # Naïve object with ignored timezone, see https://bugs.python.org/msg110249
    expiration_date: datetime
    platforms: Set[str]
    # Let's agree they are uppercased
    developer_certificate_fingerprints: Set[str]
    entitlements: Dict[str, Any]

    _mergeable_entitlements_keys = {
        "application-identifier",
        "beta-reports-active",
        "get-task-allow",
        "com.apple.developer.aps-environment",
        "com.apple.developer.team-identifier",
    }

    # See `ProvisioningProfileMetadataFactory::getAppIDFromEntitlements` from `ProvisioningProfileMetadataFactory.java` in Buck v1
    def get_app_id(self) -> AppId:
        maybe_app_id = self.entitlements.get(
            "application-identifier"
        ) or self.entitlements.get("com.apple.application-identifier")
        if not maybe_app_id:
            raise RuntimeError(
                "Entitlements do not contain app ID: {}".format(self.entitlements)
            )
        return AppId.from_string(maybe_app_id)

    # See `ProvisioningProfileMetadata::getMergeableEntitlements` from `ProvisioningProfileMetadata.java` in Buck v1
    def get_mergeable_entitlements(self) -> Dict[str, Any]:
        return {
            k: v
            for k, v in self.entitlements.items()
            if k in ProvisioningProfileMetadata._mergeable_entitlements_keys
        }

    # See `ProvisioningProfileMetadataFactory::fromProvisioningProfilePath` from `ProvisioningProfileMetadataFactory.java` in Buck v1
    @staticmethod
    def from_provisioning_profile_file_content(
        file_path: Path, content: bytes
    ) -> ProvisioningProfileMetadata:
        root = detect_format_and_loads(content)
        developer_certificate_fingerprints = {
            hashlib.sha1(c).hexdigest().upper() for c in root["DeveloperCertificates"]
        }
        assert (
            len(developer_certificate_fingerprints) > 0
        ), "Expected at least one suitable certificate."
        return ProvisioningProfileMetadata(
            file_path=file_path,
            uuid=root["UUID"],
            expiration_date=root["ExpirationDate"],
            platforms=set(root["Platform"]),
            developer_certificate_fingerprints=developer_certificate_fingerprints,
            entitlements=root["Entitlements"],
        )
