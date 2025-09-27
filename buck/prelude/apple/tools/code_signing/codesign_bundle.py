# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import logging
import os
import shutil
import subprocess
import tempfile
import uuid
from contextlib import ExitStack
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Any, Dict, List, Optional, Union

from apple.tools.plistlib_utils import detect_format_and_load

from .apple_platform import ApplePlatform
from .codesign_command_factory import (
    DefaultCodesignCommandFactory,
    DryRunCodesignCommandFactory,
    ICodesignCommandFactory,
)
from .fast_adhoc import is_fast_adhoc_codesign_allowed, should_skip_adhoc_signing_path
from .identity import CodeSigningIdentity
from .info_plist_metadata import InfoPlistMetadata
from .list_codesign_identities_command_factory import (
    IListCodesignIdentitiesCommandFactory,
    ListCodesignIdentitiesCommandFactory,
)
from .prepare_code_signing_entitlements import prepare_code_signing_entitlements
from .prepare_info_plist import prepare_info_plist
from .provisioning_profile_diagnostics import (
    interpret_provisioning_profile_diagnostics,
    META_IOS_BUILD_AND_RUN_ON_DEVICE_LINK,
    META_IOS_PROVISIONING_PROFILES_LINK,
)
from .provisioning_profile_metadata import ProvisioningProfileMetadata
from .provisioning_profile_selection import (
    CodeSignProvisioningError,
    select_best_provisioning_profile,
    SelectedProvisioningProfileInfo,
)
from .read_provisioning_profile_command_factory import (
    DefaultReadProvisioningProfileCommandFactory,
    IReadProvisioningProfileCommandFactory,
)

_default_read_provisioning_profile_command_factory = (
    DefaultReadProvisioningProfileCommandFactory()
)

_LOGGER = logging.getLogger(__name__)


def _select_provisioning_profile(
    info_plist_metadata: InfoPlistMetadata,
    provisioning_profiles_dir: Path,
    entitlements_path: Optional[Path],
    platform: ApplePlatform,
    list_codesign_identities_command_factory: IListCodesignIdentitiesCommandFactory,
    read_provisioning_profile_command_factory: IReadProvisioningProfileCommandFactory = _default_read_provisioning_profile_command_factory,
    log_file_path: Optional[Path] = None,
) -> SelectedProvisioningProfileInfo:
    identities = _list_identities(list_codesign_identities_command_factory)
    provisioning_profiles = _read_provisioning_profiles(
        provisioning_profiles_dir, read_provisioning_profile_command_factory
    )
    if not provisioning_profiles:
        raise CodeSignProvisioningError(
            f"\n\nFailed to find any provisioning profiles. Please make sure to install required provisioning profiles and make sure they are located at '{provisioning_profiles_dir}'.\n\nPlease follow the wiki to build & run on device: {META_IOS_BUILD_AND_RUN_ON_DEVICE_LINK}.\nProvisioning profiles for your app can be downloaded from {META_IOS_PROVISIONING_PROFILES_LINK}.\n"
        )
    entitlements = _read_entitlements_file(entitlements_path)
    selected_profile_info, mismatches = select_best_provisioning_profile(
        info_plist_metadata,
        identities,
        provisioning_profiles,
        entitlements,
        platform,
    )
    if selected_profile_info is None:
        if not mismatches:
            raise RuntimeError(
                f"Expected diagnostics information for at least one mismatching provisioning profile when `{provisioning_profiles_dir}` directory is not empty."
            )
        raise CodeSignProvisioningError(
            interpret_provisioning_profile_diagnostics(
                diagnostics=mismatches,
                bundle_id=info_plist_metadata.bundle_id,
                provisioning_profiles_dir=provisioning_profiles_dir,
                log_file_path=log_file_path,
            )
        )
    return selected_profile_info


@dataclass
class AdhocSigningContext:
    codesign_identity: str

    def __init__(self, codesign_identity: Optional[str] = None):
        self.codesign_identity = codesign_identity or "-"


@dataclass
class NonAdhocSigningContext:
    info_plist_source: Path
    info_plist_destination: Path
    info_plist_metadata: InfoPlistMetadata
    selected_profile_info: SelectedProvisioningProfileInfo


def non_adhoc_signing_context(
    info_plist_source: Path,
    info_plist_destination: Path,
    provisioning_profiles_dir: Path,
    entitlements_path: Optional[Path],
    platform: ApplePlatform,
    list_codesign_identities_command_factory: Optional[
        IListCodesignIdentitiesCommandFactory
    ] = None,
    log_file_path: Optional[Path] = None,
) -> NonAdhocSigningContext:
    with open(info_plist_source, mode="rb") as info_plist_file:
        info_plist_metadata = InfoPlistMetadata.from_file(info_plist_file)
    selected_profile_info = _select_provisioning_profile(
        info_plist_metadata=info_plist_metadata,
        provisioning_profiles_dir=provisioning_profiles_dir,
        entitlements_path=entitlements_path,
        platform=platform,
        list_codesign_identities_command_factory=list_codesign_identities_command_factory
        or ListCodesignIdentitiesCommandFactory.default(),
        log_file_path=log_file_path,
    )

    return NonAdhocSigningContext(
        info_plist_source,
        info_plist_destination,
        info_plist_metadata,
        selected_profile_info,
    )


# IMPORTANT: This enum is a part of incremental API, amend carefully.
class CodesignConfiguration(str, Enum):
    fastAdhoc = "fast-adhoc"
    dryRun = "dry-run"


def codesign_bundle(
    bundle_path: Path,
    signing_context: Union[AdhocSigningContext, NonAdhocSigningContext],
    entitlements_path: Optional[Path],
    platform: ApplePlatform,
    codesign_on_copy_paths: List[Path],
    codesign_tool: Optional[Path] = None,
    codesign_configuration: Optional[CodesignConfiguration] = None,
) -> None:
    with tempfile.TemporaryDirectory() as tmp_dir:
        if isinstance(signing_context, NonAdhocSigningContext):
            info_plist_metadata = signing_context.info_plist_metadata
            selected_profile_info = signing_context.selected_profile_info
            prepared_entitlements_path = prepare_code_signing_entitlements(
                entitlements_path,
                info_plist_metadata.bundle_id,
                selected_profile_info.profile,
                tmp_dir,
            )
            prepared_info_plist_path = prepare_info_plist(
                signing_context.info_plist_source,
                info_plist_metadata,
                selected_profile_info.profile,
                tmp_dir,
            )
            os.replace(
                prepared_info_plist_path,
                bundle_path / signing_context.info_plist_destination,
            )
            shutil.copy2(
                selected_profile_info.profile.file_path,
                bundle_path / platform.embedded_provisioning_profile_file_name(),
            )
            selected_identity_fingerprint = selected_profile_info.identity.fingerprint
        else:
            prepared_entitlements_path = entitlements_path
            selected_identity_fingerprint = signing_context.codesign_identity

        if codesign_configuration is CodesignConfiguration.dryRun:
            if codesign_tool is None:
                raise RuntimeError(
                    "Expected codesign tool not to be the default one when dry run codesigning is requested."
                )
            _dry_codesign_everything(
                bundle_path=bundle_path,
                codesign_on_copy_paths=codesign_on_copy_paths,
                identity_fingerprint=selected_identity_fingerprint,
                tmp_dir=tmp_dir,
                codesign_tool=codesign_tool,
                entitlements=prepared_entitlements_path,
                platform=platform,
            )
        else:
            fast_adhoc_signing_enabled = (
                codesign_configuration is CodesignConfiguration.fastAdhoc
                and is_fast_adhoc_codesign_allowed()
            )
            _LOGGER.info(f"Fast adhoc signing enabled: {fast_adhoc_signing_enabled}")
            _codesign_everything(
                bundle_path=bundle_path,
                codesign_on_copy_paths=codesign_on_copy_paths,
                identity_fingerprint=selected_identity_fingerprint,
                tmp_dir=tmp_dir,
                codesign_command_factory=DefaultCodesignCommandFactory(codesign_tool),
                entitlements=prepared_entitlements_path,
                platform=platform,
                fast_adhoc_signing=fast_adhoc_signing_enabled,
            )


def _list_identities(
    list_codesign_identities_command_factory: IListCodesignIdentitiesCommandFactory,
) -> List[CodeSigningIdentity]:
    output = subprocess.check_output(
        list_codesign_identities_command_factory.list_codesign_identities_command(),
        encoding="utf-8",
    )
    return CodeSigningIdentity.parse_security_stdout(output)


def _read_provisioning_profiles(
    dirpath: Path,
    read_provisioning_profile_command_factory: IReadProvisioningProfileCommandFactory,
) -> List[ProvisioningProfileMetadata]:
    return [
        _provisioning_profile_from_file_path(
            dirpath / f,
            read_provisioning_profile_command_factory,
        )
        for f in os.listdir(dirpath)
        if (f.endswith(".mobileprovision") or f.endswith(".provisionprofile"))
    ]


def _provisioning_profile_from_file_path(
    path: Path,
    read_provisioning_profile_command_factory: IReadProvisioningProfileCommandFactory,
) -> ProvisioningProfileMetadata:
    output = subprocess.check_output(
        read_provisioning_profile_command_factory.read_provisioning_profile_command(
            path
        ),
        stderr=subprocess.DEVNULL,
    )
    return ProvisioningProfileMetadata.from_provisioning_profile_file_content(
        path, output
    )


def _read_entitlements_file(path: Optional[Path]) -> Optional[Dict[str, Any]]:
    if not path:
        return None
    with open(path, mode="rb") as f:
        return detect_format_and_load(f)


def _dry_codesign_everything(
    bundle_path: Path,
    codesign_on_copy_paths: List[Path],
    identity_fingerprint: str,
    tmp_dir: str,
    codesign_tool: Path,
    entitlements: Optional[Path],
    platform: ApplePlatform,
) -> None:
    codesign_command_factory = DryRunCodesignCommandFactory(codesign_tool)

    codesign_on_copy_abs_paths = [bundle_path / path for path in codesign_on_copy_paths]
    codesign_on_copy_directory_paths = [
        p for p in codesign_on_copy_abs_paths if p.is_dir()
    ]

    # First sign codesign-on-copy directory paths
    _codesign_paths(
        paths=codesign_on_copy_directory_paths,
        identity_fingerprint=identity_fingerprint,
        tmp_dir=tmp_dir,
        codesign_command_factory=codesign_command_factory,
        entitlements=None,
        platform=platform,
    )

    # Dry codesigning creates a .plist inside every directory it signs.
    # That approach doesn't work for files so those files are written into .plist for root bundle.
    codesign_on_copy_file_paths = [
        p.relative_to(bundle_path) for p in codesign_on_copy_abs_paths if p.is_file()
    ]
    codesign_command_factory.set_codesign_on_copy_file_paths(
        codesign_on_copy_file_paths
    )

    # Lastly sign whole bundle
    _codesign_paths(
        paths=[bundle_path],
        identity_fingerprint=identity_fingerprint,
        tmp_dir=tmp_dir,
        codesign_command_factory=codesign_command_factory,
        entitlements=entitlements,
        platform=platform,
    )


def _codesign_everything(
    bundle_path: Path,
    codesign_on_copy_paths: List[Path],
    identity_fingerprint: str,
    tmp_dir: str,
    codesign_command_factory: ICodesignCommandFactory,
    entitlements: Optional[Path],
    platform: ApplePlatform,
    fast_adhoc_signing: bool,
) -> None:
    # First sign codesign-on-copy paths
    codesign_on_copy_filtered_paths = _filter_out_fast_adhoc_paths(
        paths=[bundle_path / path for path in codesign_on_copy_paths],
        identity_fingerprint=identity_fingerprint,
        entitlements=entitlements,
        platform=platform,
        fast_adhoc_signing=fast_adhoc_signing,
    )
    _codesign_paths(
        codesign_on_copy_filtered_paths,
        identity_fingerprint,
        tmp_dir,
        codesign_command_factory,
        None,
        platform,
    )
    # Lastly sign whole bundle
    root_bundle_paths = _filter_out_fast_adhoc_paths(
        paths=[bundle_path],
        identity_fingerprint=identity_fingerprint,
        entitlements=entitlements,
        platform=platform,
        fast_adhoc_signing=fast_adhoc_signing,
    )
    _codesign_paths(
        root_bundle_paths,
        identity_fingerprint,
        tmp_dir,
        codesign_command_factory,
        entitlements,
        platform,
    )


@dataclass
class CodesignProcess:
    process: subprocess.Popen
    stdout_path: str
    stderr_path: str

    def check_result(self) -> None:
        if self.process.returncode == 0:
            return
        with open(self.stdout_path, encoding="utf8") as stdout, open(
            self.stderr_path, encoding="utf8"
        ) as stderr:
            raise RuntimeError(
                "\nstdout:\n{}\n\nstderr:\n{}\n".format(stdout.read(), stderr.read())
            )


def _spawn_codesign_process(
    path: Path,
    identity_fingerprint: str,
    tmp_dir: str,
    codesign_command_factory: ICodesignCommandFactory,
    entitlements: Optional[Path],
    stack: ExitStack,
) -> CodesignProcess:
    stdout_path = os.path.join(tmp_dir, uuid.uuid4().hex)
    stdout = stack.enter_context(open(stdout_path, "w"))
    stderr_path = os.path.join(tmp_dir, uuid.uuid4().hex)
    stderr = stack.enter_context(open(stderr_path, "w"))
    command = codesign_command_factory.codesign_command(
        path, identity_fingerprint, entitlements
    )
    _LOGGER.info(f"Executing codesign command: {command}")
    process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
    return CodesignProcess(
        process,
        stdout_path,
        stderr_path,
    )


def _codesign_paths(
    paths: List[Path],
    identity_fingerprint: str,
    tmp_dir: str,
    codesign_command_factory: ICodesignCommandFactory,
    entitlements: Optional[Path],
    platform: ApplePlatform,
) -> None:
    """Codesigns several paths in parallel."""
    processes: List[CodesignProcess] = []
    with ExitStack() as stack:
        for path in paths:
            process = _spawn_codesign_process(
                path=path,
                identity_fingerprint=identity_fingerprint,
                tmp_dir=tmp_dir,
                codesign_command_factory=codesign_command_factory,
                entitlements=entitlements,
                stack=stack,
            )
            processes.append(process)
        for p in processes:
            p.process.wait()
    for p in processes:
        p.check_result()


def _filter_out_fast_adhoc_paths(
    paths: List[Path],
    identity_fingerprint: str,
    entitlements: Optional[Path],
    platform: ApplePlatform,
    fast_adhoc_signing: bool,
) -> List[Path]:
    if not fast_adhoc_signing:
        return paths
    # TODO(T149863217): Make skip checks run in parallel, they're usually fast (~15ms)
    # but if we have many of them (e.g., 30+ frameworks), it can add about ~0.5s.'
    return [
        p
        for p in paths
        if not should_skip_adhoc_signing_path(
            p, identity_fingerprint, entitlements, platform
        )
    ]
