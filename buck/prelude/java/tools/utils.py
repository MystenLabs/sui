# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.


import os
import pathlib
import platform
import shlex
import subprocess
import sys
import tempfile
import zipfile
from shutil import copyfile
from typing import List


def file_name_matches(file_name: str, extensions: List[str]) -> bool:
    for extension in extensions:
        if file_name.endswith(extension):
            return True
    return False


def extract_source_files(
    zipped_sources_file: pathlib.Path,
    args_file: pathlib.Path,
    file_name_extensions: List[str],
    temp_dir: tempfile.TemporaryDirectory,
) -> pathlib.Path:

    extracted_zip_dir = os.path.join(temp_dir, "extracted_srcs")
    all_extracted_files = []

    with open(zipped_sources_file) as file:
        zip_file_paths = [line.rstrip() for line in file.readlines()]
        for zip_file_path in zip_file_paths:
            with zipfile.ZipFile(zip_file_path, "r") as zip_file:
                files_to_extract = []
                for file_name in zip_file.namelist():
                    if file_name_matches(file_name, file_name_extensions):
                        files_to_extract.append(file_name)
                zip_file.extractall(path=extracted_zip_dir, members=files_to_extract)
                all_extracted_files += files_to_extract

    # append args file with new extracted sources
    merged_args_file = os.path.join(temp_dir, "merged_args_file")
    # copy content from args file
    copyfile(args_file, merged_args_file)

    with open(merged_args_file, "a") as merged_file:
        # append with extracted paths
        for path in all_extracted_files:
            merged_file.write(
                "{}{}".format(os.linesep, os.path.join(extracted_zip_dir, path))
            )
    return merged_args_file


def _to_class_name(path: pathlib.Path) -> str:
    return str(path).replace(os.sep, ".").replace(".class", "")


def sources_are_present(
    args_file: pathlib.Path, permitted_extensions: List[str]
) -> bool:
    with open(args_file, "r") as file:
        for line in file.readlines():
            for extension in permitted_extensions:
                if line.strip().endswith(extension):
                    return True
    return False


def shlex_split(cmd: str) -> List[str]:
    if platform.system() == "Windows":
        # Windows shlex.split removes backslashes.
        return cmd.split()
    else:
        return shlex.split(cmd)


def log_message(message: str):
    level = "debug"

    main_file = os.path.realpath(sys.argv[0]) if sys.argv[0] else None
    if main_file:
        program_name = os.path.basename(main_file)
    else:
        program_name = ""

    print(
        "{}[{}] {}".format(
            "[{}] ".format(program_name) if program_name else "", level, message
        ),
        file=sys.stderr,
    )


def execute_command(command: List):
    log_message(
        "executing command = '{}'".format(
            " ".join([shlex.quote(str(s)) for s in command])
        )
    )
    exit_code = subprocess.call(command)
    if exit_code != 0:
        sys.exit(exit_code)


def execute_command_ignore_exit_codes(command: List, exit_codes_to_ignore: List):
    log_message(
        "executing command = '{}'".format(
            " ".join([shlex.quote(str(s)) for s in command])
        )
    )
    exit_code = subprocess.call(command)
    if exit_code != 0 and exit_code not in exit_codes_to_ignore:
        sys.exit(exit_code)
