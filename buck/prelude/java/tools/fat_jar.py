# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.


import argparse
import os
import pathlib
import platform
import shlex
import zipfile
from shutil import copy, copytree
from tempfile import TemporaryDirectory
from typing import Dict, List

from java.tools import utils


def _parse_args():
    parser = argparse.ArgumentParser(
        description="Tool to create a fat jar from passed multiple jars."
    )

    parser.add_argument(
        "--jar_builder_tool",
        type=str,
        required=True,
        help="tool for building jars",
    )
    parser.add_argument(
        "--output", type=pathlib.Path, required=True, help="a path to an output result"
    )
    parser.add_argument(
        "--jars_file",
        type=pathlib.Path,
        required=True,
        help="paths to file with stored jars paths for merging",
    )
    parser.add_argument(
        "--main_class", type=str, help="main class to include into manifest file"
    )
    parser.add_argument(
        "--manifest", type=pathlib.Path, help="a path to a custom manifest file"
    )
    parser.add_argument(
        "--blocklist",
        type=pathlib.Path,
        help="paths to file with stored blocklist patterns",
    )
    parser.add_argument(
        "--meta_inf_directory",
        type=pathlib.Path,
        help="a path to a custom META-INF directory artifacts",
    )

    # generate wrapper params
    parser.add_argument(
        "--generate_wrapper",
        required=False,
        action="store_true",
        help="flag that if set then need to produce an executable script as a final artifact",
    )
    parser.add_argument(
        "--classpath_args_output",
        type=pathlib.Path,
        required=False,
        help="a path to an classpath args file",
    )
    parser.add_argument("--java_tool", type=pathlib.Path, help="a path to a java tool")
    parser.add_argument(
        "--script_marker_file_name",
        type=str,
        help="In case of generate script wrapper is set and native libraries are present the marker file would be stored inside the final fat jar",
    )

    # native libraries params
    parser.add_argument(
        "--native_libs_file",
        type=pathlib.Path,
        help="paths to file with stored native libs paths",
    )
    parser.add_argument(
        "--fat_jar_lib",
        type=pathlib.Path,
        help="path to fat jar lib that contains fat jar main class",
    )
    parser.add_argument(
        "--fat_jar_main_class",
        type=str,
        help="Fat jar's main class",
    )
    parser.add_argument(
        "--fat_jar_native_libs_directory_name",
        type=str,
        help="Fat jar's native libraries directory name",
    )
    parser.add_argument(
        "--do_not_create_inner_jar",
        required=False,
        action="store_true",
        help="Whether to create an inner jar if native libraries are present.",
    )

    return parser.parse_args()


def _merge_dictionaries(dict1: Dict[str, str], dict2: Dict[str, str]) -> Dict[str, str]:
    return {**dict1, **dict2}


def _shlex_split(cmd: str) -> List[str]:
    if platform.system() == "Windows":
        return cmd.split()
    else:
        return shlex.split(cmd)


# Reads a list of files from native_libs_file and symlinks each as files in native_libs_dir.
# native_libs_dir's contents are used as input to create the jar.
def build_native_libs_dir(
    native_libs_file: str, current_working_directory: str, native_libs_dir: str
) -> None:
    with open(native_libs_file) as f:
        lines = f.readlines()
        for line in lines:
            so_name, native_lib_name = line.rstrip().split(" ")
            native_lib_path = os.path.join(current_working_directory, native_lib_name)
            os.symlink(
                native_lib_path,
                os.path.join(native_libs_dir, so_name),
            )


def main():
    args = _parse_args()

    jar_builder_tool = args.jar_builder_tool
    output_path = args.output
    jars_file = args.jars_file
    main_class = args.main_class
    manifest = args.manifest
    blocklist_file = args.blocklist
    meta_inf_directory = args.meta_inf_directory

    generate_wrapper = args.generate_wrapper
    classpath_args_output = args.classpath_args_output
    java_tool = args.java_tool
    script_marker_file_name = args.script_marker_file_name

    native_libs_file = args.native_libs_file
    do_not_create_inner_jar = args.do_not_create_inner_jar
    fat_jar_lib = args.fat_jar_lib
    fat_jar_main_class = args.fat_jar_main_class
    fat_jar_native_libs_directory_name = args.fat_jar_native_libs_directory_name

    utils.log_message("jar_builder_tool: {}".format(jar_builder_tool))
    utils.log_message("output: {}".format(output_path))
    utils.log_message("jars_file: {}".format(jars_file))

    if native_libs_file:
        utils.log_message("native_libs_file: {}".format(native_libs_file))
        utils.log_message("fat_jar_lib: {}".format(fat_jar_lib))
        utils.log_message("fat_jar_main_class: {}".format(fat_jar_main_class))
        utils.log_message(
            "fat_jar_native_libs_directory_name: {}".format(
                fat_jar_native_libs_directory_name
            )
        )
        utils.log_message("do_not_create_inner_jar: {}".format(do_not_create_inner_jar))

    if main_class:
        utils.log_message("main_class = {}".format(main_class))
    if manifest:
        utils.log_message("manifest = {}".format(manifest))
    if blocklist_file:
        utils.log_message("blocklist_file = {}".format(blocklist_file))
    if meta_inf_directory:
        utils.log_message("meta_inf_directory = {}".format(meta_inf_directory))
    if generate_wrapper:
        utils.log_message("generate_wrapper = {}".format(generate_wrapper))
        utils.log_message("classpath_args_output: {}".format(classpath_args_output))
        utils.log_message("java_tool: {}".format(java_tool))
        utils.log_message("script_marker_file_name: {}".format(script_marker_file_name))

    need_to_process_native_libs = native_libs_file is not None
    if need_to_process_native_libs and not do_not_create_inner_jar:
        if (
            fat_jar_lib is None
            or fat_jar_main_class is None
            or fat_jar_native_libs_directory_name is None
        ):
            raise AssertionError(
                "All native libraries inner jar related params have to be present!"
            )
    else:
        if (
            fat_jar_lib is not None
            or fat_jar_main_class is not None
            or fat_jar_native_libs_directory_name is not None
        ):
            raise AssertionError(
                "All native libraries inner jar related params should not be present!"
            )

    if generate_wrapper is True:
        if (
            classpath_args_output is None
            or java_tool is None
            or script_marker_file_name is None
        ):
            raise AssertionError(
                "All generate wrapper related params have to be present!"
            )
    else:
        if (
            classpath_args_output is not None
            or java_tool is not None
            or script_marker_file_name is not None
        ):
            raise AssertionError(
                "All generate wrapper related params have to be present!"
            )

    current_working_directory = os.getcwd()
    with TemporaryDirectory() as temp_dir:
        if generate_wrapper:  # generate wrapper script
            jars = []
            with open(jars_file) as f:
                lines = f.readlines()
                for line in lines:
                    jars.append(line.rstrip())
            utils.log_message("jars: {}".format(jars))
            with open(classpath_args_output, "w") as f:
                f.write(":".join(jars))

            jar_output = output_path
            with open(jar_output, "w") as f:
                script_args = [
                    str(java_tool),
                    "-cp",
                    "@" + str(classpath_args_output),
                ]
                if main_class:
                    script_args.append(main_class)
                script_args.append('"$@"')

                f.write(" ".join(script_args))

        else:  # generate fat jar

            jar_cmd = []
            jar_cmd.extend(utils.shlex_split(jar_builder_tool))

            if need_to_process_native_libs and do_not_create_inner_jar:
                # symlink native libs to `nativelibs` directory
                native_libs_staging = pathlib.Path(temp_dir) / "native_libs_staging"
                native_libs_staging.mkdir()
                native_libs_staging_subdir = (
                    pathlib.Path(temp_dir) / "native_libs_staging" / "nativelibs"
                )
                native_libs_staging_subdir.mkdir()
                build_native_libs_dir(
                    native_libs_file=native_libs_file,
                    current_working_directory=current_working_directory,
                    native_libs_dir=native_libs_staging_subdir,
                )
                jars_and_native_libs_directory_file = (
                    pathlib.Path(temp_dir) / "jars_and_nativelibs_directory_file"
                )
                # combine jars_file and native_libs_file into a single set of entries
                with open(jars_and_native_libs_directory_file, "w") as f:
                    with open(jars_file, "r") as f2:
                        f.write(str(f2.read()) + "\n")
                    f.write(str(native_libs_staging))

                jar_cmd.extend(
                    ["--entries-to-jar", jars_and_native_libs_directory_file]
                )
            else:
                jar_cmd.extend(["--entries-to-jar", jars_file])

            if meta_inf_directory:
                meta_inf_staging = pathlib.Path(temp_dir) / "meta_inf_staging"
                meta_inf_staging.mkdir()
                copytree(
                    meta_inf_directory,
                    meta_inf_staging / "META-INF",
                    copy_function=copy,
                    dirs_exist_ok=True,
                )

                meta_inf_directory_file = (
                    pathlib.Path(temp_dir) / "meta_inf_directory_file"
                )
                with open(meta_inf_directory_file, "w") as f:
                    f.write(str(meta_inf_staging))

                jar_cmd.extend(["--override-entries-to-jar", meta_inf_directory_file])

            if main_class:
                jar_cmd.extend(["--main-class", main_class])

            if blocklist_file:
                jar_cmd.extend(["--blocklist-patterns", blocklist_file])
                jar_cmd.extend(["--blocklist-patterns-matcher", "substring"])

            if manifest:
                jar_cmd.extend(["--manifest-file", manifest])

            jar_cmd.append("--merge-manifests")

            jar_output = (
                os.path.join(temp_dir, "inner.jar")
                if need_to_process_native_libs and not do_not_create_inner_jar
                else output_path
            )
            jar_cmd.extend(["--output", jar_output])
            utils.log_message("jar_cmd: {}".format(jar_cmd))
            utils.execute_command(jar_cmd)

        if need_to_process_native_libs and not do_not_create_inner_jar:
            fat_jar_content_dir = os.path.join(temp_dir, "fat_jar_content_dir")
            os.mkdir(fat_jar_content_dir)

            # copy inner.jar into an appropriate location for packaging
            inner_jar_file = os.path.join(fat_jar_content_dir, "inner.jar")
            if generate_wrapper:
                copy(
                    jar_output,
                    inner_jar_file,
                )
                # marker file used in FatJarMain.java main class to correctly launch a new jvm process with native libs.
                marker_file = os.path.join(fat_jar_content_dir, script_marker_file_name)
                pathlib.Path(marker_file).touch()
            else:
                os.symlink(
                    jar_output,
                    inner_jar_file,
                )

            # symlink native libs to `nativelibs` directory
            native_libs_dir = os.path.join(
                fat_jar_content_dir, fat_jar_native_libs_directory_name
            )
            os.mkdir(native_libs_dir)

            build_native_libs_dir(
                native_libs_file=native_libs_file,
                current_working_directory=current_working_directory,
                native_libs_dir=native_libs_dir,
            )

            # Build the final fat JAR from the structure we've laid out above.  We first package the
            # fat jar resources (e.g. native libs) using the "stored" compression level, to avoid
            # expensive compression on builds and decompression on startup.
            contents_zip_path = os.path.join(temp_dir, "contents.zip")
            with zipfile.ZipFile(
                contents_zip_path,
                mode="w",
                strict_timestamps=False,
                compression=zipfile.ZIP_STORED,
            ) as contents_zip:
                for content in pathlib.Path(fat_jar_content_dir).rglob("*"):
                    contents_zip.write(
                        content,
                        content.relative_to(fat_jar_content_dir),
                    )

            entries_to_jar_file = os.path.join(temp_dir, "entries_to_jar.txt")
            with open(entries_to_jar_file, "w") as f:
                f.write("\n".join([contents_zip_path, str(fat_jar_lib)]))

            fat_jar_cmd = []
            fat_jar_cmd.extend(utils.shlex_split(jar_builder_tool))
            fat_jar_cmd.extend(["--main-class", fat_jar_main_class])
            fat_jar_cmd.extend(["--output", output_path])
            fat_jar_cmd.extend(["--entries-to-jar", entries_to_jar_file])
            fat_jar_cmd.append("--merge-manifests")
            utils.execute_command(fat_jar_cmd)


if __name__ == "__main__":
    main()
