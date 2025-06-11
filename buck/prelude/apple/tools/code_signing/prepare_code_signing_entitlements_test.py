# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import os
import plistlib
import tempfile
import unittest
from datetime import datetime
from pathlib import Path

from .prepare_code_signing_entitlements import prepare_code_signing_entitlements
from .provisioning_profile_metadata import ProvisioningProfileMetadata


class Test(unittest.TestCase):
    def test_minimal_entitlements_generated_based_on_provisioning_profile(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            profile = ProvisioningProfileMetadata(
                Path("/foo"),
                "00000000-0000-0000-0000-000000000000",
                datetime.max,
                {"iOS"},
                {},
                {
                    "application-identifier": "ABCDEFGHIJ.*",
                    "com.apple.developer.aps-environment": "development",
                },
            )
            result = prepare_code_signing_entitlements(
                None, "com.company.application", profile, tmp_dir
            )
            with open(result, mode="rb") as result_file:
                self.assertEqual(
                    plistlib.load(result_file),
                    {
                        "application-identifier": "ABCDEFGHIJ.com.company.application",
                        "com.apple.developer.aps-environment": "development",
                        "keychain-access-groups": [
                            "ABCDEFGHIJ.com.company.application"
                        ],
                    },
                )

    def test_entitlements_enriched_by_profile(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            entitlements = {"foo": "bar"}
            entitlements_path = os.path.join(tmp_dir, "Entitlements.plist")
            with open(entitlements_path, mode="wb") as entitlements_file:
                plistlib.dump(entitlements, entitlements_file, fmt=plistlib.FMT_XML)
            profile = ProvisioningProfileMetadata(
                Path("/foo"),
                "00000000-0000-0000-0000-000000000000",
                datetime.max,
                {"iOS"},
                {},
                {
                    "application-identifier": "ABCDEFGHIJ.com.company.application",
                    "com.apple.developer.aps-environment": "development",
                    "should.be.ignored": "dummy",
                },
            )
            result = prepare_code_signing_entitlements(
                entitlements_path, "com.company.application", profile, tmp_dir
            )
            with open(result, "rb") as result_file:
                self.assertEqual(
                    plistlib.load(result_file),
                    {
                        "foo": "bar",
                        "application-identifier": "ABCDEFGHIJ.com.company.application",
                        "com.apple.developer.aps-environment": "development",
                    },
                )
