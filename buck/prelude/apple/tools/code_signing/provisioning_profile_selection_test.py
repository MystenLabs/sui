# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import copy
import unittest
from datetime import datetime
from pathlib import Path
from typing import List

from .apple_platform import ApplePlatform
from .identity import CodeSigningIdentity
from .info_plist_metadata import InfoPlistMetadata
from .provisioning_profile_diagnostics import IProvisioningProfileDiagnostics
from .provisioning_profile_metadata import ProvisioningProfileMetadata
from .provisioning_profile_selection import (
    select_best_provisioning_profile,
    SelectedProvisioningProfileInfo,
)


class TestSelection(unittest.TestCase):
    def verify_diagnostic_info_candidate_profile(
        self,
        diagnostic_info: List[IProvisioningProfileDiagnostics],
        reason: str,
    ):
        self.assertEqual(len(diagnostic_info), 1)
        candidate_profile = diagnostic_info[0]
        self.assertEqual(
            candidate_profile.log_message(),
            reason,
        )

    def test_expired_profiles_are_ignored(self):
        info_plist = InfoPlistMetadata("com.company.application", None, False)
        identity = CodeSigningIdentity(
            "fingerprint",
            "name",
        )
        expired_provisioning_profile = ProvisioningProfileMetadata(
            Path("/foo"),
            "00000000-0000-0000-0000-000000000000",
            datetime.min,
            {"iOS"},
            {identity.fingerprint},
            {"application-identifier": "ABCDEFGHIJ.com.company.application"},
        )
        selected, diagnostic_info = select_best_provisioning_profile(
            info_plist,
            [identity],
            [expired_provisioning_profile],
            {},
            ApplePlatform.ios_device,
        )
        self.assertIsNone(selected)
        self.verify_diagnostic_info_candidate_profile(
            diagnostic_info,
            "Provisioning profile expired.",
        )

        fresh_provisioning_profiles = copy.copy(expired_provisioning_profile)
        fresh_provisioning_profiles.expiration_date = datetime.max
        selected, _ = select_best_provisioning_profile(
            info_plist,
            [identity],
            [fresh_provisioning_profiles],
            {},
            ApplePlatform.ios_device,
        )
        self.assertIsNotNone(selected)

    def test_prefix_override(self):
        info_plist = InfoPlistMetadata("com.company.application", None, False)
        identity = CodeSigningIdentity(
            "fingerprint",
            "name",
        )
        expected = ProvisioningProfileMetadata(
            Path("/foo"),
            "00000000-0000-0000-0000-000000000000",
            datetime.max,
            {"iOS"},
            {identity.fingerprint},
            {"application-identifier": "AAAAAAAAAA.*"},
        )
        profiles = [
            expected,
            ProvisioningProfileMetadata(
                Path("/foo"),
                "00000000-0000-0000-0000-000000000000",
                datetime.max,
                {"iOS"},
                {identity.fingerprint},
                {"application-identifier": "BBBBBBBBBB.com.company.application"},
            ),
        ]
        selected, _ = select_best_provisioning_profile(
            info_plist,
            [identity],
            profiles,
            {"keychain-access-groups": ["AAAAAAAAAA.*"]},
            ApplePlatform.ios_device,
        )
        self.assertEqual(selected, SelectedProvisioningProfileInfo(expected, identity))

    def test_entitlement_keys_are_matched(self):
        info_plist = InfoPlistMetadata("com.company.application", None, False)
        identity = CodeSigningIdentity(
            "fingerprint",
            "name",
        )
        expected = ProvisioningProfileMetadata(
            Path("/foo"),
            "11111111-1111-1111-1111-111111111111",
            datetime.max,
            {"iOS"},
            {identity.fingerprint},
            {
                "application-identifier": "AAAAAAAAAA.com.company.application",
                "keychain-access-groups": ["AAAAAAAAAA.*"],
                "aps-environment": "production",
                "com.apple.security.application-groups": ["foo", "bar", "baz"],
            },
        )
        unexpected = ProvisioningProfileMetadata(
            Path("/foo"),
            "00000000-0000-0000-0000-000000000000",
            datetime.max,
            {"iOS"},
            {identity.fingerprint},
            {
                "application-identifier": "AAAAAAAAAA.com.company.application",
                "keychain-access-groups": ["AAAAAAAAAA.*"],
                "aps-environment": "development",
                "com.apple.security.application-groups": ["foo", "bar"],
            },
        )
        profiles = [
            expected,
            unexpected,
        ]
        selected, _ = select_best_provisioning_profile(
            info_plist,
            [identity],
            profiles,
            {
                "keychain-access-groups": ["AAAAAAAAAA.*"],
                "aps-environment": "production",
                "com.apple.security.application-groups": ["foo", "bar"],
            },
            ApplePlatform.ios_device,
        )
        self.assertEqual(selected, SelectedProvisioningProfileInfo(expected, identity))

        selected, _ = select_best_provisioning_profile(
            info_plist,
            [identity],
            profiles,
            {
                "aps-environment": "production",
                "com.apple.security.application-groups": ["foo", "bar"],
            },
            ApplePlatform.ios_device,
        )
        self.assertEqual(selected, SelectedProvisioningProfileInfo(expected, identity))

        selected, diagnostic_info = select_best_provisioning_profile(
            info_plist,
            [identity],
            [unexpected],
            {
                "aps-environment": "production",
                "com.apple.security.application-groups": ["foo", "xxx"],
            },
            ApplePlatform.ios_device,
        )
        self.assertIsNone(selected)
        self.verify_diagnostic_info_candidate_profile(
            diagnostic_info,
            "Expected entitlement item key `aps-environment` with value `production` not found in provisioning profile.",
        )

    def test_only_profiles_containing_valid_fingerprints_are_matched(self):
        info_plist = InfoPlistMetadata("com.company.application", None, False)
        valid_identity = CodeSigningIdentity(
            "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            "iPhone Developer: Foo Bar (54321EDCBA)",
        )
        other_identity = CodeSigningIdentity(
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "iPhone Developer: Foo Bar (ABCDE12345)",
        )
        expected = ProvisioningProfileMetadata(
            Path("/foo"),
            "11111111-1111-1111-1111-111111111111",
            datetime.max,
            {"iOS"},
            {valid_identity.fingerprint, other_identity.fingerprint},
            {
                "application-identifier": "AAAAAAAAAA.*",
            },
        )
        unexpected = ProvisioningProfileMetadata(
            Path("/foo"),
            "00000000-0000-0000-0000-000000000000",
            datetime.max,
            {"iOS"},
            {other_identity.fingerprint},
            {
                "application-identifier": "AAAAAAAAAA.com.company.application",
            },
        )

        profiles = [expected, unexpected]
        selected, _ = select_best_provisioning_profile(
            info_plist,
            [valid_identity],
            profiles,
            {},
            ApplePlatform.ios_device,
        )
        self.assertEqual(
            selected, SelectedProvisioningProfileInfo(expected, valid_identity)
        )
        selected, diagnostic_info = select_best_provisioning_profile(
            info_plist,
            [valid_identity],
            [unexpected],
            {},
            ApplePlatform.ios_device,
        )
        self.assertIsNone(selected)
        self.verify_diagnostic_info_candidate_profile(
            diagnostic_info,
            "Expected identity fingerprint not found in profile's certificate fingerprints `AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA`.",
        )

    def test_matches_specific_app(self):
        info_plist = InfoPlistMetadata("com.facebook.test", None, False)
        identity = CodeSigningIdentity(
            "fingerprint",
            "name",
        )
        expected = ProvisioningProfileMetadata(
            Path("/foo"),
            "00000000-0000-0000-0000-000000000000",
            datetime.max,
            {"iOS"},
            {identity.fingerprint},
            {
                "application-identifier": "BBBBBBBBBB.com.facebook.test",
            },
        )
        profiles = [
            expected,
            ProvisioningProfileMetadata(
                Path("/foo"),
                "11111111-1111-1111-1111-111111111111",
                datetime.max,
                {"iOS"},
                {identity.fingerprint},
                {
                    "application-identifier": "BBBBBBBBBB.com.facebook.*",
                },
            ),
        ]
        selected, _ = select_best_provisioning_profile(
            info_plist,
            [identity],
            profiles,
            {},
            ApplePlatform.ios_device,
        )
        self.assertEqual(selected, SelectedProvisioningProfileInfo(expected, identity))

        selected, _ = select_best_provisioning_profile(
            info_plist,
            [identity],
            reversed(profiles),
            {},
            ApplePlatform.ios_device,
        )
        self.assertEqual(selected, SelectedProvisioningProfileInfo(expected, identity))

    def test_matches_wildcard(self):
        info_plist = InfoPlistMetadata("com.facebook.test", None, False)
        identity = CodeSigningIdentity(
            "fingerprint",
            "name",
        )
        expected = ProvisioningProfileMetadata(
            Path("/foo"),
            "00000000-0000-0000-0000-000000000000",
            datetime.max,
            {"iOS"},
            {identity.fingerprint},
            {
                "application-identifier": "BBBBBBBBBB.*",
            },
        )
        selected, _ = select_best_provisioning_profile(
            info_plist,
            [identity],
            [expected],
            None,
            ApplePlatform.ios_device,
        )
        self.assertEqual(selected, SelectedProvisioningProfileInfo(expected, identity))

    def test_force_included_app_entitlements(self):
        info_plist = InfoPlistMetadata("com.facebook.test", None, False)
        identity = CodeSigningIdentity(
            "fingerprint",
            "name",
        )
        profile = ProvisioningProfileMetadata(
            Path("/foo"),
            "00000000-0000-0000-0000-000000000000",
            datetime.max,
            {"iOS"},
            {identity.fingerprint},
            {
                "application-identifier": "AAAAAAAAAA.com.facebook.test",
                "keychain-access-groups": ["AAAAAAAAAA.*"],
                "aps-environment": "production",
            },
        )
        selected, _ = select_best_provisioning_profile(
            info_plist,
            [identity],
            [profile],
            {
                # Force included key, even if not present in the profile
                "application-identifier": "AAAAAAAAAA.com.facebook.BuckApp",
                "keychain-access-groups": ["AAAAAAAAAA.*"],
                "aps-environment": "production",
            },
            ApplePlatform.ios_device,
        )
        self.assertIsNotNone(selected)

    def test_unmatched_app_entitlement(self):
        info_plist = InfoPlistMetadata("com.facebook.test", None, False)
        identity = CodeSigningIdentity(
            "fingerprint",
            "name",
        )
        profile = ProvisioningProfileMetadata(
            Path("/foo"),
            "00000000-0000-0000-0000-000000000000",
            datetime.max,
            {"iOS"},
            {identity.fingerprint},
            {
                "application-identifier": "AAAAAAAAAA.com.facebook.test",
                "keychain-access-groups": ["AAAAAAAAAA.*"],
                "aps-environment": "production",
            },
        )
        selected, diagnostic_info = select_best_provisioning_profile(
            info_plist,
            [identity],
            [profile],
            {
                "keychain-access-groups": ["AAAAAAAAAA.*"],
                "aps-environment": "production",
                "com.made.up.entitlement": "buck",
            },
            ApplePlatform.ios_device,
        )
        self.assertIsNone(selected)
        self.verify_diagnostic_info_candidate_profile(
            diagnostic_info,
            "Expected entitlement item key `com.made.up.entitlement` with value `buck` not found in provisioning profile.",
        )
