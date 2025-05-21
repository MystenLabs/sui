# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import datetime
import logging
from dataclasses import dataclass
from typing import Any, cast, Dict, List, Optional, Tuple

from .app_id import AppId
from .apple_platform import ApplePlatform
from .identity import CodeSigningIdentity
from .info_plist_metadata import InfoPlistMetadata
from .provisioning_profile_diagnostics import (
    BundleIdMismatch,
    DeveloperCertificateMismatch,
    EntitlementsMismatch,
    IProvisioningProfileDiagnostics,
    ProfileExpired,
    TeamIdMismatch,
    UnsupportedPlatform,
)
from .provisioning_profile_metadata import ProvisioningProfileMetadata

_LOGGER = logging.getLogger(__name__)


class CodeSignProvisioningError(Exception):
    pass


def _parse_team_id_from_entitlements(
    entitlements: Optional[Dict[str, Any]]
) -> Optional[str]:
    if not entitlements:
        return None
    maybe_app_id = AppId.infer_from_entitlements(entitlements)
    if not maybe_app_id:
        return None
    return maybe_app_id.team_id


def _matches_or_array_is_subset_of(
    entitlement_name: str,
    expected_value: Any,
    actual_value: Any,
    platform: ApplePlatform,
) -> bool:
    if expected_value is None:
        return actual_value is None
    if (
        actual_value is None
        and platform.is_desktop()
        and entitlement_name.startswith("com.apple.security")
    ):
        # For macOS apps, including Catalyst, the provisioning profile would _not_ have entries for
        # the sandbox entitlements, so any value matches.
        return True
    if isinstance(expected_value, list) and isinstance(actual_value, list):
        return set(expected_value).issubset(set(actual_value))
    return actual_value == expected_value


def _bundle_match_length(expected_bundle_id: str, bundle_id_pattern: str) -> int:
    if bundle_id_pattern.endswith("*"):
        # Chop the ending * if wildcard.
        bundle_id_without_wildcard = bundle_id_pattern[:-1]
        if expected_bundle_id.startswith(bundle_id_without_wildcard):
            return len(bundle_id_without_wildcard)
    elif expected_bundle_id == bundle_id_pattern:
        return len(bundle_id_pattern)
    return -1


# For those keys let the tooling decide if code signing should fail or succeed (every other key
# mismatch results in provisioning profile being skipped).
_IGNORE_MISMATCH_ENTITLEMENTS_KEYS = {
    "keychain-access-groups",
    "application-identifier",
    "com.apple.developer.associated-domains",
    "com.apple.developer.icloud-container-development-container-identifiers",
    "com.apple.developer.icloud-container-environment",
    "com.apple.developer.icloud-container-identifiers",
    "com.apple.developer.icloud-services",
    "com.apple.developer.ubiquity-container-identifiers",
    "com.apple.developer.ubiquity-kvstore-identifier",
}


def _check_entitlements_match(
    expected_entitlements: Optional[Dict[str, Any]],
    profile: ProvisioningProfileMetadata,
    platform: ApplePlatform,
    bundle_id_match_length: int,
) -> Tuple[bool, Optional[EntitlementsMismatch]]:
    if expected_entitlements is None:
        return (True, None)
    for (key, value) in expected_entitlements.items():
        profile_entitlement = profile.entitlements.get(key)
        if (key not in _IGNORE_MISMATCH_ENTITLEMENTS_KEYS) and (
            not _matches_or_array_is_subset_of(
                key, value, profile_entitlement, platform
            )
        ):
            return (
                False,
                EntitlementsMismatch(
                    profile=profile,
                    bundle_id_match_length=bundle_id_match_length,
                    mismatched_key=key,
                    mismatched_value=value,
                ),
            )
    return (True, None)


def _check_developer_certificates_match(
    profile: ProvisioningProfileMetadata,
    identities: List[CodeSigningIdentity],
    bundle_id_match_length: int,
) -> Tuple[Optional[CodeSigningIdentity], Optional[DeveloperCertificateMismatch]]:
    for identity in identities:
        if identity.fingerprint in profile.developer_certificate_fingerprints:
            return (identity, None)
    return (
        None,
        DeveloperCertificateMismatch(
            profile=profile, bundle_id_match_length=bundle_id_match_length
        ),
    )


@dataclass
class SelectedProvisioningProfileInfo:
    profile: ProvisioningProfileMetadata
    identity: CodeSigningIdentity


# See `ProvisioningProfileStore::getBestProvisioningProfile` in `ProvisioningProfileStore.java` for Buck v1 equivalent
def select_best_provisioning_profile(
    info_plist_metadata: InfoPlistMetadata,
    code_signing_identities: List[CodeSigningIdentity],
    provisioning_profiles: List[ProvisioningProfileMetadata],
    entitlements: Optional[Dict[str, Any]],
    platform: ApplePlatform,
) -> Tuple[
    Optional[SelectedProvisioningProfileInfo], List[IProvisioningProfileDiagnostics]
]:
    """Selects the best provisioning profile and certificate to use when code signing the bundle.
       Such profile could be successfully used to sign the bundle taking into account
       different constraints like entitlements or bundle ID. Given several profiles
       could be successfully used to sign the bundle the "best" one is considered
       to be a profile which matches bundle ID  from `Info.plist` the most
       (i.e. profiles with specific bundle ID are preferred to wildcard bundle IDs).
    Args:
       info_plist_metadata: Object representing `Info.plist` file in the bundle.
       code_signing_identities: Code signing identities to choose from.
       provisioning_profiles: Provisioning profiles to choose from.
       entitlements: Code signing entitlements if any.
       platform: Apple platform which the bundle is built for.
    Returns:
       Provisioning profile and certificate pair to use for code signing.
    """
    maybe_team_id_constraint = _parse_team_id_from_entitlements(entitlements)

    best_match_length = -1
    result = None

    # Used for error messages
    diagnostics = []

    def log_mismatched_profile(mismatch: IProvisioningProfileDiagnostics) -> None:
        diagnostics.append(mismatch)
        _LOGGER.info(
            f"Skipping provisioning profile `{mismatch.profile.file_path.name}`: {mismatch.log_message()}"
        )

    for profile in provisioning_profiles:
        app_id = profile.get_app_id()
        if maybe_team_id_constraint and maybe_team_id_constraint != app_id.team_id:
            log_mismatched_profile(
                TeamIdMismatch(
                    profile=profile,
                    team_id=app_id.team_id,
                    team_id_constraint=maybe_team_id_constraint,
                )
            )
            continue

        bundle_id = app_id.bundle_id
        current_match_length = _bundle_match_length(
            info_plist_metadata.bundle_id, bundle_id
        )
        if current_match_length < 0:
            log_mismatched_profile(
                BundleIdMismatch(
                    profile=profile,
                    bundle_id=app_id.bundle_id,
                    bundle_id_constraint=info_plist_metadata.bundle_id,
                )
            )
            continue

        if datetime.datetime.now() > profile.expiration_date:
            log_mismatched_profile(
                ProfileExpired(
                    profile=profile, bundle_id_match_length=current_match_length
                )
            )
            continue

        maybe_provisioning_profile_name = platform.provisioning_profile_name()
        if (
            maybe_provisioning_profile_name
            and maybe_provisioning_profile_name not in profile.platforms
        ):
            log_mismatched_profile(
                UnsupportedPlatform(
                    profile=profile,
                    bundle_id_match_length=current_match_length,
                    platform_constraint=platform,
                )
            )
            continue

        entitlements_matched, mismatch = _check_entitlements_match(
            expected_entitlements=entitlements,
            profile=profile,
            platform=platform,
            bundle_id_match_length=current_match_length,
        )
        if not entitlements_matched:
            log_mismatched_profile(cast(EntitlementsMismatch, mismatch))
            continue

        certificate, mismatch = _check_developer_certificates_match(
            profile=profile,
            identities=code_signing_identities,
            bundle_id_match_length=current_match_length,
        )
        if not certificate:
            log_mismatched_profile(cast(DeveloperCertificateMismatch, mismatch))
            continue

        if current_match_length > best_match_length:
            best_match_length = current_match_length
            result = SelectedProvisioningProfileInfo(profile, certificate)

    return result, diagnostics
