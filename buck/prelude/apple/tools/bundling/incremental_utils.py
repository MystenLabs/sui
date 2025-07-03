# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import logging
import os
from pathlib import Path
from typing import Dict, List, Set, Tuple

from .assemble_bundle_types import BundleSpecItem, IncrementalContext
from .incremental_state import IncrementalStateItem


def should_assemble_incrementally(
    spec: List[BundleSpecItem], incremental_context: IncrementalContext
) -> bool:
    previous_run_state = incremental_context.state
    if previous_run_state is None:
        logging.getLogger(__name__).info(
            "Decided not to assemble incrementally — no incremental state for previous build."
        )
        return False
    previously_codesigned = previous_run_state.codesigned
    # If previously bundle was not code signed there should be no problems with code signing
    # currently in incremental mode. Existing binaries could be code signed "on
    # top" if that's needed.
    if not previously_codesigned:
        logging.getLogger(__name__).info(
            "Decided to assemble incrementally — previous build is not codesigned."
        )
        return True
    # For simplicity and correctness purposes instead of stripping code signatures we
    # perform non-incremental run.
    if not incremental_context.codesigned:
        logging.getLogger(__name__).info(
            "Decided not to assemble incrementally — current build is not codesigned, while previous build is codesigned."
        )
        return False
    # If previous identity is different from the current one also perform non-incremental run.
    if previous_run_state.codesign_identity != incremental_context.codesign_identity:
        logging.getLogger(__name__).info(
            "Decided not to assemble incrementally — previous vs current builds have mismatching codesigning identities."
        )
        return False
    # If bundle from previous run was signed in a different configuration vs the current run (e.g. dry code signed while now regular code signing is required) perform non-incremental run.
    if (
        previous_run_state.codesign_configuration
        != incremental_context.codesign_configuration
    ):
        logging.getLogger(__name__).info(
            "Decided not to assemble incrementally — previous vs current builds have mismatching codesigning configurations."
        )
        return False
    # If there is an artifact that was code signed on copy in previous run which is
    # present in current run and not code signed on copy, we should perform
    # non-incremental run for simplicity and correctness reasons.
    current_codesigned_on_copy_paths = {Path(i.dst) for i in spec if i.codesign_on_copy}
    codesigned_on_copy_paths_from_previous_build_which_are_present_in_current_build = _codesigned_on_copy_paths_from_previous_build_which_are_present_in_current_build(
        set(previous_run_state.codesign_on_copy_paths),
        {Path(i.dst) for i in spec},
    )
    codesign_on_copy_paths_are_compatible = codesigned_on_copy_paths_from_previous_build_which_are_present_in_current_build.issubset(
        current_codesigned_on_copy_paths
    )
    if not codesign_on_copy_paths_are_compatible:
        logging.getLogger(__name__).info(
            f"Decided not to assemble incrementally — there is at least one artifact `{list(codesigned_on_copy_paths_from_previous_build_which_are_present_in_current_build - current_codesigned_on_copy_paths)[0]}` that was code signed on copy in previous build which is present in current run and not code signed on copy."
        )
    return codesign_on_copy_paths_are_compatible


def _codesigned_on_copy_paths_from_previous_build_which_are_present_in_current_build(
    previously_codesigned_on_copy_paths: Set[Path],
    all_input_files: Set[Path],
):
    all_input_files_and_directories = all_input_files | {
        i for file in all_input_files for i in file.parents
    }
    return previously_codesigned_on_copy_paths & all_input_files_and_directories


def _get_new_digest(action_metadata: Dict[Path, str], path: Path) -> str:
    # While a resource file can be in a symlinked folder, like the `ghi/def` example below,
    # ```
    # project_root
    #           ├── abc
    #           │    └── def
    #           └── ghi -> abc
    # ```
    # In this case, Python would say `ghi/abc` not a symlink. However the `action_metadata` comes
    # with the actual resolved path (`abc/def`). We need to resolve the path then.
    # Given Python doesn't think it's a symlink, the `readlink` API wouldn't work either
    resolved_path = path.resolve().relative_to(Path.cwd())
    return action_metadata[resolved_path]


def calculate_incremental_state(
    spec: List[BundleSpecItem], action_metadata: Dict[Path, str]
) -> List[IncrementalStateItem]:
    """
    `action_metadata` maps Buck project relative paths to hash digest
    for every input file of the action which executes this script
    """
    result = []
    source_with_destination_files = _source_with_destination_files(spec)
    for (src, dst) in source_with_destination_files:
        is_symlink = src.is_symlink()
        new_digest = _get_new_digest(action_metadata, src) if not is_symlink else None
        resolved_symlink = Path(os.readlink(src)) if is_symlink else None
        result.append(
            IncrementalStateItem(
                source=src,
                destination_relative_to_bundle=dst,
                digest=new_digest,
                resolved_symlink=resolved_symlink,
            )
        )
    return result


def _source_with_destination_files(
    spec: List[BundleSpecItem],
) -> List[Tuple[Path, Path]]:
    """
    Returns:
        Ordered mapping from source path to destination path (relative to bundle) for every file
        present in a bundle. Directories that were parts of the spec are split into actual files.
    """
    result = []
    for spec_item in spec:
        file_or_dir = Path(spec_item.src)
        if file_or_dir.is_file():
            if not spec_item.dst:
                raise RuntimeError(
                    f'Invalid input bundle spec. File located at {file_or_dir} should not have `""` destination (only directories are allowed to have such value).'
                )
            result.append((file_or_dir, Path(spec_item.dst)))
        elif file_or_dir.is_dir():
            result.extend(
                [
                    (file, spec_item.dst / file.relative_to(file_or_dir))
                    for file in _list_directory_deterministically(file_or_dir)
                ]
            )
        else:
            raise RuntimeError(
                f"Path {file_or_dir} is not a file and not a dir, don't know how to handle it."
            )
    return result


def _list_directory_deterministically(directory: Path) -> List[Path]:
    result = []
    for current_dir_path, dir_names, file_names in os.walk(directory):
        result += [Path(os.path.join(current_dir_path, f)) for f in sorted(file_names)]
        # Sort in order for walk to be deterministic.
        dir_names.sort()
    return result
