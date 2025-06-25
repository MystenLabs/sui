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

from .info_plist_metadata import InfoPlistMetadata
from .prepare_info_plist import prepare_info_plist
from .provisioning_profile_metadata import ProvisioningProfileMetadata


class Test(unittest.TestCase):
    def test_app_id_set_for_non_watchos_apps(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            profile = ProvisioningProfileMetadata(
                Path("/foo"),
                "00000000-0000-0000-0000-000000000000",
                datetime.max,
                {"iOS"},
                {},
                {
                    "application-identifier": "ABCDEFGHIJ.*",
                },
            )
            info_plist = {
                "CFBundleIdentifier": "com.facebook.test",
                "CFBundlePackageType": "APPL",
            }
            info_plist_path, info_plist_metadata = _write_info_plist(
                info_plist, tmp_dir, "Info.plist"
            )
            result = prepare_info_plist(
                info_plist_path, info_plist_metadata, profile, tmp_dir
            )
            with open(result, "rb") as result_file:
                self.assertEqual(
                    plistlib.load(result_file),
                    {
                        "CFBundleIdentifier": "com.facebook.test",
                        "CFBundlePackageType": "APPL",
                        "ApplicationIdentifier": "ABCDEFGHIJ.com.facebook.test",
                    },
                )
            # Same but for watchOS Info.plist
            info_plist = {
                "CFBundleIdentifier": "com.facebook.test",
                "CFBundlePackageType": "APPL",
                "WKWatchKitApp": True,
            }
            info_plist_path, info_plist_metadata = _write_info_plist(
                info_plist, tmp_dir, "Info.plist"
            )
            result = prepare_info_plist(
                info_plist_path, info_plist_metadata, profile, tmp_dir
            )
            with open(result, "rb") as result_file:
                self.assertNotIn("ApplicationIdentifier", plistlib.load(result_file))


def _write_info_plist(plist, tmp_dir, name):
    path = os.path.join(tmp_dir, "Info.plist")
    with open(path, mode="wb") as file:
        plistlib.dump(plist, file, fmt=plistlib.FMT_XML)
    with open(path, mode="rb") as file:
        metadata = InfoPlistMetadata.from_file(file)
        return (path, metadata)
