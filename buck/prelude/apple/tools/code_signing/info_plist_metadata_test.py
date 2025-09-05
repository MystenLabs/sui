# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import io
import unittest

from .info_plist_metadata import InfoPlistMetadata


class TestParse(unittest.TestCase):
    def test_canary(self):
        plist = io.BytesIO(
            b"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.company.application</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>WKWatchKitApp</key>
    <false/>
</dict>
</plist>
"""
        )
        expected = InfoPlistMetadata("com.company.application", "APPL", False)
        result = InfoPlistMetadata.from_file(plist)
        self.assertEqual(expected, result)

    def test_not_watch_application_by_default(self):
        plist = io.BytesIO(
            b"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.company.application</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
</dict>
</plist>
"""
        )
        expected = InfoPlistMetadata("com.company.application", "APPL", False)
        result = InfoPlistMetadata.from_file(plist)
        self.assertEqual(expected, result)

    def test_package_type_can_be_omitted(self):
        plist = io.BytesIO(
            b"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.company.application</string>
</dict>
</plist>
"""
        )
        expected = InfoPlistMetadata("com.company.application", None, False)
        result = InfoPlistMetadata.from_file(plist)
        self.assertEqual(expected, result)
