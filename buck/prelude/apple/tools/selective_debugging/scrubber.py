# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import json

import os
import shutil
import subprocess
from typing import BinaryIO, Callable, List, Optional, Set, Tuple

from apple.tools.re_compatibility_utils.writable import make_path_user_writable

from .macho import Symbol

from .macho_parser import load_commands, load_debug_symbols, load_header
from .spec import Spec
from .utils import MachOException


FAKE_PATH = b"fake/path"
# buck-out/isolation_dir/gen/project_cell/{hash}/....
NUM_OF_COMPONENTS_IN_BUCK2_OUTPUT_PATH_BEFORE_PROJECT_PATH = 5


def _always_scrub(_: str) -> bool:
    return True


# Visible for testing
def load_focused_targets_output_paths(json_file_path: str) -> Set[str]:
    if json_file_path is None or not os.path.exists(json_file_path):
        return set()
    with open(json_file_path, "r") as f:
        content = f.read()
        if not content:
            return set()
        data = json.loads(content)
        output_paths = set()
        for target in data["targets"]:
            _, package_and_name = target.split("//")
            package, name = package_and_name.split(":")
            # This assumes the output path created by buck2, which if
            # modified, would break this logic.
            output_directory = f"{package}/__{name}__"
            output_paths.add(output_directory)
    return output_paths


# This function converts buck-out/isolation_dir/gen/project_cell/{hash}/X/Y/__name__/libFoo.a
# into X/Y/__name__ to match the focus target output path created by load_focused_targets_output_paths
# Visible for testing
def _get_target_output_path_from_debug_file_path(
    debug_target_path: str,
):
    # This function assumes the debug file path created by buck2 is in the following format:
    # buck-out/isolation_dir/gen/project_cell/{hash}/.../__name__/libFoo.a
    parts = debug_target_path.split("/")

    # We are doing the traverse in reverse order because this way we'll find the first
    # target directory sooner. _should_scrub can get called many times, so it's
    # important that we make it as efficient as possible.
    i = 1
    while i <= len(parts):
        if parts[-i].startswith("__") and parts[-i].endswith("__"):
            break
        i += 1
    if i > len(parts):
        raise Exception(
            f"Unrecognized format for debug file path : {debug_target_path}"
        )

    return "/".join(
        parts[NUM_OF_COMPONENTS_IN_BUCK2_OUTPUT_PATH_BEFORE_PROJECT_PATH : -i + 1]
    )


# Visible for testing
def should_scrub_with_focused_targets_output_paths(
    focused_targets_output_paths: Set[str], debug_file_path: str
) -> bool:
    # All paths to be scrubbed when no focused target is specified
    if len(focused_targets_output_paths) == 0:
        return True

    # debug_file_path usually have the format x/y/z/libFoo.a(bar.m.o)
    if "(" in debug_file_path:
        debug_target_path = debug_file_path.split("(")[0]
    else:
        debug_target_path = debug_file_path

    if debug_file_path.startswith("buck-out/"):
        target_output_path = _get_target_output_path_from_debug_file_path(
            debug_target_path
        )
        return target_output_path not in focused_targets_output_paths
    else:
        # occasionally archive file can be directly from source.
        (package, name) = os.path.split(debug_file_path)
        while package != "":
            if f"{package}/__{name}__" in focused_targets_output_paths:
                return False
            (package, name) = os.path.split(package)

        return True


def _should_scrub_with_targets_file(json_file_path: str) -> Callable[[str], bool]:
    focused_targets_output_paths = load_focused_targets_output_paths(json_file_path)
    return lambda debug_file_path: should_scrub_with_focused_targets_output_paths(
        focused_targets_output_paths, debug_file_path
    )


def _should_scrub_with_spec_file(json_file_path: str) -> Callable[[str], bool]:
    spec = Spec(json_file_path)
    return spec.scrub_debug_file_path


def _scrub(
    f: BinaryIO,
    strtab_offset: int,
    symbols: List[Symbol],
    scrub_handler: Callable[[str], bool],
) -> List[Tuple[str, str]]:
    """
    Return a list of tuples.
    Each tuple contains a pair of the original path and the rewritten path
    """
    results = []
    for symbol in symbols:
        f.seek(strtab_offset)
        f.seek(symbol.strtab_index, 1)

        # Read a byte at a time until we reach the end of the path, denoted by Hex 0.
        start = end = f.tell()
        path = b""
        b = f.read(1)
        while b != b"\x00":
            path += b
            end += 1
            b = f.read(1)
        str_len = end - start

        path_str = path.decode()
        if scrub_handler(path_str):
            f.seek(start)
            # We don't want to modify the length of the path, so pad the replacement
            # path with spaces
            buffer = FAKE_PATH + b" " * (str_len - len(FAKE_PATH))
            f.write(buffer)
            results.append((path_str, buffer.decode()))
        else:
            results.append((path_str, path_str))
    return results


def scrub(
    input_file: str,
    output_file: str,
    targets_file: Optional[str] = None,
    spec_file: Optional[str] = None,
    adhoc_codesign_tool: Optional[str] = None,
) -> List[Tuple[str, str]]:
    if targets_file and spec_file:
        raise Exception(
            "Only one of a targets file or spec file is supported, not both!"
        )
    elif targets_file:
        scrub_handler = _should_scrub_with_targets_file(targets_file)
    elif spec_file:
        scrub_handler = _should_scrub_with_spec_file(spec_file)
    else:
        scrub_handler = _always_scrub

    shutil.copy2(input_file, output_file)
    # Make it RE-compatible
    make_path_user_writable(output_file)

    results = []
    with open(output_file, "r+b") as f:
        header, offset = load_header(f, 0)
        if not header.is_valid:
            raise MachOException("Invalid macho format!")
        lc_linkedit, lc_symtab = load_commands(f, offset, header.n_cmds)
        if lc_linkedit is None:
            return []
        if lc_symtab is None:
            raise MachOException("LC_SYMTAB command not found")
        if lc_symtab.strtab_size == 0 or lc_symtab.n_symbols == 0:
            return []
        if lc_linkedit.file_size == 0:
            raise MachOException("LC_SEGMENT_64 command for string table not found")
        f.seek(lc_symtab.strtab_offset)
        """
        ld64 deliberately burns the first byte with the space character, so that zero is never a
        valid string index and writes 0x00 at offset 1, so that it's always the empty string.
        The code for this in ld64 is in LinkEditClassic.hpp (StringPoolAtom::StringPoolAtom).
        """
        if f.read(1) != b"\x20":
            raise MachOException("First character in the string table is not a space")
        if f.read(1) != b"\x00":
            raise MachOException("Second character in the string table is not a NUL")

        symbols = load_debug_symbols(f, lc_symtab.symtab_offset, lc_symtab.n_symbols)
        results = _scrub(f, lc_symtab.strtab_offset, symbols, scrub_handler)

    if adhoc_codesign_tool:
        subprocess.run(
            [adhoc_codesign_tool, "--binary", output_file],
            check=True,
        )

    return results
