# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import os
import tempfile
from pathlib import Path
from typing import Any, Dict

from apple.tools.info_plist_processor.process import process as process_info_plist

from .info_plist_metadata import InfoPlistMetadata
from .provisioning_profile_metadata import ProvisioningProfileMetadata

# Buck v1 corresponding code is in `ProvisioningProfileCopyStep::execute` in `ProvisioningProfileCopyStep.java`
def prepare_info_plist(
    info_plist: Path,
    info_plist_metadata: InfoPlistMetadata,
    profile: ProvisioningProfileMetadata,
    tmp_dir: str,
) -> Path:
    fd, output_path = tempfile.mkstemp(dir=tmp_dir)
    with open(info_plist, "rb") as input, os.fdopen(fd, mode="wb") as output:
        additional_keys = _additional_keys(info_plist_metadata, profile)
        process_info_plist(
            input_file=input, output_file=output, additional_keys=additional_keys
        )
    return Path(output_path)


# Equivalent Buck v1 code is in `ProvisioningProfileCopyStep.java` in `ProvisioningProfileCopyStep::getInfoPlistAdditionalKeys` method.
def _additional_keys(
    info_plist_metadata: InfoPlistMetadata, profile: ProvisioningProfileMetadata
) -> Dict[str, Any]:
    result = {}
    # Restrict additional keys based on bundle type. Skip additional keys for watchOS bundles (property keys whitelist).
    if (
        info_plist_metadata.bundle_type == "APPL"
        and not info_plist_metadata.is_watchos_app
    ):
        # Construct AppID using the Provisioning Profile info (app prefix)
        app_id = profile.get_app_id().team_id + "." + info_plist_metadata.bundle_id
        result["ApplicationIdentifier"] = app_id
    return result
