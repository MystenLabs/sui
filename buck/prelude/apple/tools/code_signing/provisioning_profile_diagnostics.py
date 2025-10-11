# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

from abc import ABCMeta, abstractmethod
from pathlib import Path
from typing import List, Optional, Type, TypeVar

from .apple_platform import ApplePlatform

from .provisioning_profile_metadata import ProvisioningProfileMetadata

META_IOS_DEVELOPER_CERTIFICATE_LINK: str = "https://www.internalfb.com/intern/qa/5198/how-do-i-get-the-fb-ios-developer-certificate"
META_IOS_PROVISIONING_PROFILES_LINK: str = (
    "https://www.internalfb.com/intern/apple/download-provisioning-profile/"
)
META_IOS_BUILD_AND_RUN_ON_DEVICE_LINK: str = "https://www.internalfb.com/intern/wiki/Ios-first-steps/running-on-device/#2-register-your-device-i"


class IProvisioningProfileDiagnostics(metaclass=ABCMeta):
    profile: ProvisioningProfileMetadata

    def __init__(self, profile: ProvisioningProfileMetadata):
        self.profile = profile

    @abstractmethod
    def log_message(self) -> str:
        raise NotImplementedError


class TeamIdMismatch(IProvisioningProfileDiagnostics):
    team_id: str
    team_id_constraint: str

    def __init__(
        self,
        profile: ProvisioningProfileMetadata,
        team_id: str,
        team_id_constraint: str,
    ):
        super().__init__(profile)
        self.team_id = team_id
        self.team_id_constraint = team_id_constraint

    def log_message(self) -> str:
        return f"Profile team ID `{self.team_id}` is not matching constraint `{self.team_id_constraint}`"


class BundleIdMismatch(IProvisioningProfileDiagnostics):
    bundle_id: str
    bundle_id_constraint: str

    def __init__(
        self,
        profile: ProvisioningProfileMetadata,
        bundle_id: str,
        bundle_id_constraint: str,
    ):
        super().__init__(profile)
        self.bundle_id = bundle_id
        self.bundle_id_constraint = bundle_id_constraint

    def log_message(self) -> str:
        return f"Bundle ID `{self.bundle_id}` is not matching constraint `{self.bundle_id_constraint}`"


class ProfileExpired(IProvisioningProfileDiagnostics):
    bundle_id_match_length: int

    def __init__(
        self,
        profile: ProvisioningProfileMetadata,
        bundle_id_match_length: int,
    ):
        super().__init__(profile)
        self.bundle_id_match_length = bundle_id_match_length

    def log_message(self) -> str:
        return "Provisioning profile expired."


class UnsupportedPlatform(IProvisioningProfileDiagnostics):
    bundle_id_match_length: int
    platform_constraint: str

    def __init__(
        self,
        profile: ProvisioningProfileMetadata,
        bundle_id_match_length: int,
        platform_constraint: ApplePlatform,
    ):
        super().__init__(profile)
        self.bundle_id_match_length = bundle_id_match_length
        self.platform_constraint = platform_constraint

    def log_message(self) -> str:
        supported_profile_platforms = ", ".join(self.profile.platforms)
        return f"Requested platform `{self.platform_constraint}` is not in provisioning profile's supported platforms `{supported_profile_platforms}`."


class EntitlementsMismatch(IProvisioningProfileDiagnostics):
    bundle_id_match_length: int
    mismatched_key: str
    mismatched_value: str

    def __init__(
        self,
        profile: ProvisioningProfileMetadata,
        bundle_id_match_length: int,
        mismatched_key: str,
        mismatched_value: str,
    ):
        super().__init__(profile)
        self.bundle_id_match_length = bundle_id_match_length
        self.mismatched_key = mismatched_key
        self.mismatched_value = mismatched_value

    def log_message(self) -> str:
        return f"Expected entitlement item key `{self.mismatched_key}` with value `{self.mismatched_value}` not found in provisioning profile."


class DeveloperCertificateMismatch(IProvisioningProfileDiagnostics):
    bundle_id_match_length: int

    def __init__(
        self,
        profile: ProvisioningProfileMetadata,
        bundle_id_match_length: int,
    ):
        super().__init__(profile)
        self.bundle_id_match_length = bundle_id_match_length

    def log_message(self) -> str:
        certificate_fingerprints = ", ".join(
            self.profile.developer_certificate_fingerprints
        )
        return f"Expected identity fingerprint not found in profile's certificate fingerprints `{certificate_fingerprints}`."


_T = TypeVar("T")


def interpret_provisioning_profile_diagnostics(
    diagnostics: List[IProvisioningProfileDiagnostics],
    bundle_id: str,
    provisioning_profiles_dir: Path,
    log_file_path: Optional[Path] = None,
) -> str:
    if not diagnostics:
        raise RuntimeError(
            "Expected diagnostics information for at least one mismatching provisioning profile."
        )

    header = f"Failed to find provisioning profile in directory `{provisioning_profiles_dir}` that is suitable for code signing. Here is the best guess for how to fix it:\n\n⚠️  "
    footer = f"\n\nFor more info about running on an iOS device read {META_IOS_BUILD_AND_RUN_ON_DEVICE_LINK}."
    if log_file_path:
        footer += (
            f" Full list of mismatched profiles can be found at `{log_file_path}`.\n"
        )
    else:
        provisioning_profile_errors = "\n\n".join(
            [
                f"`{mismatch.profile.file_path.name}`: {mismatch.log_message()}"
                for mismatch in diagnostics
            ]
        )
        footer += f" Full list of mismatched profiles:{provisioning_profile_errors}\n"

    def find_mismatch(class_type: Type[_T]) -> Optional[_T]:
        return next(
            iter(
                sorted(
                    filter(lambda d: isinstance(d, class_type), diagnostics),
                    key=lambda d: d.bundle_id_match_length,
                    reverse=True,
                )
            ),
            None,
        )

    if mismatch := find_mismatch(DeveloperCertificateMismatch):
        return "".join(
            [
                header,
                f"The provisioning profile `{mismatch.profile.file_path.name}` satisfies all constraints, but no matching certificates were found in your keychain. ",
                f"Please download and install the latest certificate from {META_IOS_DEVELOPER_CERTIFICATE_LINK}.",
                footer,
            ]
        )

    if mismatch := find_mismatch(EntitlementsMismatch):
        return "".join(
            [
                header,
                f"The provisioning profile `{mismatch.profile.file_path.name}` is the best match, but it doesn't contain all the needed entitlements. ",
                f"Expected entitlement item with key `{mismatch.mismatched_key}` and value `{mismatch.mismatched_value}` is missing. ",
                f"Usually that means the application entitlements were changed recently, provisioning profile was updated and you need to download & install the latest version of provisioning profile for Bundle ID `{bundle_id}` from {META_IOS_PROVISIONING_PROFILES_LINK}",
                footer,
            ]
        )

    if mismatch := find_mismatch(UnsupportedPlatform):
        supported_platforms = ", ".join([f"`{p}`" for p in mismatch.profile.platforms])
        return "".join(
            [
                header,
                f"The provisioning profile `{mismatch.profile.file_path.name}` is the best match, but it only supports the following platforms: {supported_platforms}. Unfortunately, it doesn't include the requested platform, `{mismatch.platform_constraint}`. ",
                f"This could indicate two possibilities: either the provisioning profile was recently updated to include the needed platform, or there is a separate profile supporting the required platform that you are missing. In the latter case, you would need to download and install it from {META_IOS_PROVISIONING_PROFILES_LINK}",
                footer,
            ]
        )

    if mismatch := find_mismatch(ProfileExpired):
        return "".join(
            [
                header,
                f"The provisioning profile `{mismatch.profile.file_path.name}` is the the best match; however, it has expired",
                f"Please download and install a valid profile from {META_IOS_PROVISIONING_PROFILES_LINK}",
                footer,
            ]
        )

    return "".join(
        [
            header,
            f"No provisioning profile matching the Bundle ID `{bundle_id}` was found. ",
            f"Please download and install the appropriate profile from {META_IOS_PROVISIONING_PROFILES_LINK}",
            footer,
        ]
    )
