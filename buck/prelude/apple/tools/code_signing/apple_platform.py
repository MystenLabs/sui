# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

from enum import Enum
from typing import Optional


class ApplePlatform(str, Enum):
    macos = "macosx"
    ios_device = "iphoneos"
    ios_simulator = "iphonesimulator"
    watch_device = "watchos"
    watch_simulator = "watchsimulator"
    tv_device = "appletvos"
    tv_simulator = "appletvsimulator"
    mac_catalyst = "maccatalyst"
    driver_kit = "driverkit"

    def is_desktop(self) -> bool:
        return self == ApplePlatform.macos or self == ApplePlatform.mac_catalyst

    def provisioning_profile_name(self) -> Optional[str]:
        """
        Returns:
           The platform name as it could be found inside provisioning profiles and used to match them.
           Not all platforms use provisioning profiles, those will return `None`.
        """
        if self == ApplePlatform.ios_device or self == ApplePlatform.watch_device:
            return "iOS"
        elif self == ApplePlatform.tv_device:
            return "tvOS"
        else:
            return None

    def embedded_provisioning_profile_file_name(self) -> str:
        """
        Returns:
           The name of the provisioning profile in the final application bundle.
        """
        if self.is_desktop():
            return "embedded.provisionprofile"
        else:
            return "embedded.mobileprovision"
