# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import logging
import os
import shutil
from pathlib import Path
from typing import cast, Dict, List, Optional

from .assemble_bundle_types import BundleSpecItem, IncrementalContext
from .incremental_state import IncrementalState, IncrementalStateItem
from .incremental_utils import (
    calculate_incremental_state,
    should_assemble_incrementally,
)

_LOGGER = logging.getLogger(__name__)


def assemble_bundle(
    spec: List[BundleSpecItem],
    bundle_path: Path,
    incremental_context: Optional[IncrementalContext],
    check_conflicts: bool,
) -> Optional[List[IncrementalStateItem]]:
    # It's possible to have the same spec multiple times as different
    # apple_resource() targets can refer to the _same_ resource file.
    #
    # On RE, we're not allowed to overwrite files, so prevent doing
    # identical file copies.
    #
    # Do not reorder spec items to achieve determinism.
    # Rely on the fact that `dict` preserves key order.
    deduplicated_spec = list(dict.fromkeys(spec))
    # Force same sorting as in Buck1 for `SourcePathWithAppleBundleDestination`
    # WARNING: This logic is tightly coupled with how spec filtering is done in `_filter_conflicting_paths` method during incremental bundling. Don't change unless you fully understand what is going on here.
    deduplicated_spec.sort()

    incremental_result = None
    if incremental_context:
        if should_assemble_incrementally(deduplicated_spec, incremental_context):
            incremental_result = _assemble_incrementally(
                bundle_path,
                deduplicated_spec,
                incremental_context.metadata,
                cast(IncrementalState, incremental_context.state),
                check_conflicts,
            )
        else:
            _assemble_non_incrementally(bundle_path, deduplicated_spec, check_conflicts)
            incremental_result = calculate_incremental_state(
                deduplicated_spec, incremental_context.metadata
            )
    else:
        _assemble_non_incrementally(bundle_path, deduplicated_spec, check_conflicts)

    # External tooling (e.g., Xcode) might depend on the timestamp of the bundle
    bundle_path.touch()

    return incremental_result


def _cleanup_output(incremental: bool, path: Path) -> None:
    if not incremental and path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True, exist_ok=True)


def _assemble_non_incrementally(
    bundle_path: Path, spec: List[BundleSpecItem], check_conflicts: bool
) -> None:
    logging.getLogger(__name__).info("Assembling bundle non-incrementally.")
    _cleanup_output(incremental=False, path=bundle_path)

    copied_contents = {}

    def _copy(src, dst, **kwargs) -> None:
        if check_conflicts:
            if dst in copied_contents:
                raise RuntimeError(
                    f"Found a conflict for destination `{os.path.relpath(dst, bundle_path)}`: `{src}` conflicts with `{copied_contents[dst]}`"
                )
        shutil.copy2(src, dst, follow_symlinks=False)
        if check_conflicts:
            copied_contents[dst] = src

    for spec_item in spec:
        source_path = spec_item.src
        destination_path = bundle_path / spec_item.dst

        destination_path.parent.mkdir(parents=True, exist_ok=True)
        if os.path.isdir(source_path):
            shutil.copytree(
                source_path,
                destination_path,
                symlinks=True,
                dirs_exist_ok=True,
                copy_function=_copy,
            )
        else:
            _copy(source_path, destination_path)


def _assemble_incrementally(
    bundle_path: Path,
    spec: List[BundleSpecItem],
    action_metadata: Dict[Path, str],
    incremental_state: IncrementalState,
    check_conflicts: bool,
) -> List[IncrementalStateItem]:
    logging.getLogger(__name__).info("Assembling bundle incrementally.")
    _cleanup_output(incremental=True, path=bundle_path)
    _delete_swift_stdlib_files(bundle_path, incremental_state.swift_stdlib_paths)
    paths_to_delete = {
        i.destination_relative_to_bundle for i in incremental_state.items
    }
    old_digests = {
        item.destination_relative_to_bundle: item.digest
        for item in incremental_state.items
        if item.digest is not None
    }
    old_symlink_destinations = {
        item.destination_relative_to_bundle: item.resolved_symlink
        for item in incremental_state.items
        if item.resolved_symlink is not None
    }

    new_incremental_state = calculate_incremental_state(spec, action_metadata)

    if check_conflicts:
        _check_path_conflicts(new_incremental_state)
    else:
        new_incremental_state = _filter_conflicting_paths(new_incremental_state)

    for item in new_incremental_state:
        # Added file might not be present in old result, need to check first.
        dst = item.destination_relative_to_bundle
        if dst in paths_to_delete:
            paths_to_delete.remove(dst)
        project_relative_dst = bundle_path / dst
        if item.digest is not None:
            new_digest = item.digest
            old_digest = old_digests.get(dst, None)
            is_changed = new_digest != old_digest
        else:
            assert (
                item.resolved_symlink is not None
            ), "Expected item to represent a symlink when digest is missing"
            new_resolved_symlink = item.resolved_symlink
            old_resolved_symlink = old_symlink_destinations.get(dst, None)
            is_changed = new_resolved_symlink != old_resolved_symlink
            if is_changed:
                project_relative_dst.unlink()
        if is_changed:
            _LOGGER.debug(
                f"Bundle item at path `{dst}` changed, updating with `{item.source}`."
            )
            project_relative_dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(item.source, project_relative_dst, follow_symlinks=False)

    for path in paths_to_delete:
        (bundle_path / path).unlink()

    _cleanup_empty_redundant_directories(
        bundle_path, incremental_state.items, new_incremental_state
    )

    return new_incremental_state


def _check_path_conflicts(incremental_state: List[IncrementalStateItem]) -> None:
    checked = {}
    for item in incremental_state:
        dst = item.destination_relative_to_bundle
        if dst in checked:
            raise RuntimeError(
                f"Found a conflict for destination `{dst}`: `{item.source}` conflicts with `{checked[dst]}`"
            )
        checked[dst] = item.source


def _filter_conflicting_paths(
    incremental_state: List[IncrementalStateItem],
) -> List[IncrementalStateItem]:
    """
    Filter out conflicting paths leaving only the last item from the conflicting items. That practically means that the last item overrides all other conflicting items which makes:
    1) incremental build deterministic even when there are multiple conflicting destination paths
    2) bundling result has the same structure as in Buck1 even when there are multiple conflicting destination paths
    WARNING: This logic is tightly coupled with how spec items are sorted in `assemble_bundle` method. Don't change unless you fully understand what is going on here.
    """
    result = {}
    for item in incremental_state:
        dst = item.destination_relative_to_bundle
        # Keep the same ordering of elements as in incremental state.
        # That means we don't just overwrite the item for existing key, but need to delete it first.
        if dst in result:
            result.pop(dst)
        result[dst] = item
    return list(result.values())


def _delete_swift_stdlib_files(
    bundle_path: Path, swift_stdlib_paths: List[Path]
) -> None:
    for p in swift_stdlib_paths:
        path = bundle_path / p
        if path.is_dir():
            shutil.rmtree(path)
        else:
            path.unlink(missing_ok=True)


def _cleanup_empty_redundant_directories(
    bundle_path: Path,
    old_state: List[IncrementalStateItem],
    new_state: List[IncrementalStateItem],
) -> None:
    old_directories = {
        p for item in old_state for p in item.destination_relative_to_bundle.parents
    }
    new_directories = {
        p for item in new_state for p in item.destination_relative_to_bundle.parents
    }
    for redundant_directory in old_directories - new_directories:
        shutil.rmtree(bundle_path / redundant_directory, ignore_errors=True)
