# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import os
import plistlib
import tempfile
from pathlib import Path
from typing import Optional

from apple.tools.info_plist_processor.process import process as process_info_plist

from .provisioning_profile_metadata import ProvisioningProfileMetadata

# Buck v1 corresponding code is in `ProvisioningProfileCopyStep::execute` in `ProvisioningProfileCopyStep.java`
def prepare_code_signing_entitlements(
    entitlements_path: Optional[Path],
    bundle_id: str,
    profile: ProvisioningProfileMetadata,
    tmp_dir: str,
) -> Path:
    fd, output_path = tempfile.mkstemp(dir=tmp_dir)
    with os.fdopen(fd, mode="wb") as output:
        if entitlements_path:
            with open(entitlements_path, "rb") as entitlements_file:
                process_info_plist(
                    input_file=entitlements_file,
                    output_file=output,
                    additional_keys=profile.get_mergeable_entitlements(),
                    output_format=plistlib.FMT_XML,
                )
        else:
            app_id = profile.get_app_id().team_id + "." + bundle_id
            entitlements = profile.get_mergeable_entitlements()
            entitlements["application-identifier"] = app_id
            entitlements["keychain-access-groups"] = [app_id]
            plistlib.dump(entitlements, output, fmt=plistlib.FMT_XML)
    return Path(output_path)
