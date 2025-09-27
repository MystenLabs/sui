# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.


import argparse
import os
import pathlib
from tempfile import TemporaryDirectory

from java.tools import utils

_JAVA_FILE_EXTENSION = [".java"]


def _parse_args():
    parser = argparse.ArgumentParser(
        description="Tool to compile and create a jar for java library."
    )

    parser.add_argument(
        "--jar_builder_tool",
        type=str,
        required=True,
        help="a path to a jar builder tool",
    )
    parser.add_argument(
        "--output", type=pathlib.Path, required=True, help="a path to an output result"
    )
    parser.add_argument(
        "--skip_javac_run",
        action="store_true",
        help="flag that if set then need to skip javac execution",
    )
    parser.add_argument(
        "--javac_tool",
        type=pathlib.Path,
        help="a path to a java compiler tool",
    )
    parser.add_argument(
        "--javac_args_file",
        type=pathlib.Path,
        required=False,
        metavar="javac_args_file",
        help="path to file with stored args that need to be passed to java compiler",
    )
    parser.add_argument(
        "--zipped_sources_file",
        type=pathlib.Path,
        required=False,
        metavar="zipped_sources_file",
        help="path to file with stored zipped source files that need to be passed to java compiler",
    )

    parser.add_argument(
        "--javac_classpath_file",
        type=pathlib.Path,
        required=False,
        metavar="javac_classpath_file",
        help="path to file with stored classpath for java compilation",
    )
    parser.add_argument(
        "--javac_processors_classpath_file",
        type=pathlib.Path,
        required=False,
        metavar="javac_processors_classpath_file",
        help="path to file with stored classpath for java compilation processors",
    )
    parser.add_argument(
        "--javac_bootclasspath_file",
        type=pathlib.Path,
        required=False,
        metavar="javac_bootclasspath_file",
        help="path to file with stored bootclasspath for java compilation",
    )
    parser.add_argument(
        "--resources_dir",
        type=pathlib.Path,
        required=False,
        metavar="resources_dir",
        help="path to a directory with resources",
    )
    parser.add_argument(
        "--generated_sources_dir",
        type=pathlib.Path,
        required=False,
        metavar="generated_sources_dir",
        help="path to a directory where generated sources should be written",
    )
    parser.add_argument(
        "--manifest",
        type=pathlib.Path,
        required=False,
        metavar="manifest",
        help="a path to a custom manifest file",
    )
    parser.add_argument(
        "--remove_classes",
        type=pathlib.Path,
        help="paths to file with stored remove classes patterns",
    )
    parser.add_argument(
        "--additional_compiled_srcs",
        type=pathlib.Path,
        required=False,
        metavar="additional_compiled_srcs",
        help=".class files that should be packaged into the final jar",
    )

    return parser.parse_args()


def _run_javac(
    javac_tool: pathlib.Path,
    javac_args_file: pathlib.Path,
    zipped_sources_file: pathlib.Path,
    javac_classpath_file: pathlib.Path,
    javac_processor_classpath_file: pathlib.Path,
    javac_bootclasspath_file: pathlib.Path,
    generated_sources_dir: pathlib.Path,
    temp_dir: TemporaryDirectory,
) -> pathlib.Path:
    javac_output = os.path.join(temp_dir, "classes")
    os.mkdir(javac_output)

    javac_cmd = [javac_tool]

    args_file = javac_args_file
    if zipped_sources_file:
        args_file = utils.extract_source_files(
            zipped_sources_file, javac_args_file, _JAVA_FILE_EXTENSION, temp_dir
        )

    if utils.sources_are_present(args_file, _JAVA_FILE_EXTENSION):
        javac_cmd += ["@{}".format(args_file)]

        if javac_classpath_file:
            javac_cmd += ["-classpath", "@{}".format(javac_classpath_file)]

        if javac_bootclasspath_file:
            javac_cmd += ["-bootclasspath", "@{}".format(javac_bootclasspath_file)]

        if javac_processor_classpath_file:
            javac_cmd += [
                "-processorpath",
                "@{}".format(javac_processor_classpath_file),
            ]

        if generated_sources_dir:
            javac_cmd += [
                "-s",
                generated_sources_dir,
            ]

        javac_cmd += ["-g"]

        javac_cmd += ["-d", javac_output]
        utils.execute_command(javac_cmd)

    return pathlib.Path(javac_output)


def _run_jar(
    jar_builder_tool: str,
    output_path: pathlib.Path,
    manifest: pathlib.Path,
    javac_output: pathlib.Path,
    resources_dir: pathlib.Path,
    additional_compiled_srcs: pathlib.Path,
    remove_classes_file: pathlib.Path,
    temp_dir: TemporaryDirectory,
):
    jar_cmd = []
    jar_cmd.extend(utils.shlex_split(jar_builder_tool))

    content_to_pack_dirs = []
    if javac_output:
        content_to_pack_dirs.append(javac_output)
    if resources_dir:
        content_to_pack_dirs.append(resources_dir)
    if additional_compiled_srcs:
        content_to_pack_dirs.append(additional_compiled_srcs)

    entries_to_jar_file = pathlib.Path(temp_dir) / "entries_to_jar.txt"
    with open(entries_to_jar_file, "w") as f:
        f.write("\n".join([str(path) for path in content_to_pack_dirs]))

    jar_cmd.extend(["--entries-to-jar", entries_to_jar_file])

    if manifest:
        jar_cmd.extend(["--manifest-file", manifest])

    if remove_classes_file:
        jar_cmd.extend(["--blocklist-patterns", remove_classes_file])
        jar_cmd.extend(
            ["--blocklist-patterns-matcher", "remove_classes_patterns_matcher"]
        )

    jar_cmd.extend(["--output", output_path])

    utils.log_message("jar_cmd: {}".format(" ".join([str(s) for s in jar_cmd])))
    utils.execute_command(jar_cmd)


def main():
    args = _parse_args()

    skip_javac_run = args.skip_javac_run
    javac_tool = args.javac_tool
    jar_builder_tool = args.jar_builder_tool
    output_path = args.output
    javac_args = args.javac_args_file
    zipped_sources_file = args.zipped_sources_file
    javac_classpath = args.javac_classpath_file
    javac_processor_classpath = args.javac_processors_classpath_file
    javac_bootclasspath_file = args.javac_bootclasspath_file
    resources_dir = args.resources_dir
    generated_sources_dir = args.generated_sources_dir
    manifest = args.manifest
    remove_classes_file = args.remove_classes
    additional_compiled_srcs = args.additional_compiled_srcs

    utils.log_message("javac_tool: {}".format(javac_tool))
    utils.log_message("jar_builder_tool: {}".format(jar_builder_tool))
    utils.log_message("output: {}".format(output_path))
    if skip_javac_run:
        utils.log_message("skip_javac_run: {}".format(skip_javac_run))
    if javac_args:
        utils.log_message("javac_args: {}".format(javac_args))
    if zipped_sources_file:
        utils.log_message("zipped_sources_file: {}".format(zipped_sources_file))
    if javac_classpath:
        utils.log_message("javac_classpath: {}".format(javac_classpath))
    if javac_processor_classpath:
        utils.log_message(
            "javac_processor_classpath: {}".format(javac_processor_classpath)
        )
    if javac_bootclasspath_file:
        utils.log_message(
            "javac_bootclasspath_file: {}".format(javac_bootclasspath_file)
        )
    if resources_dir:
        utils.log_message("resources_dir: {}".format(resources_dir))
    if generated_sources_dir:
        utils.log_message("generated_sources_dir: {}".format(generated_sources_dir))
        if not generated_sources_dir.exists():
            generated_sources_dir.mkdir()
    if manifest:
        utils.log_message("manifest: {}".format(manifest))
    if remove_classes_file:
        utils.log_message("remove classes file: {}".format(remove_classes_file))
    if additional_compiled_srcs:
        utils.log_message(
            "additional_compiled_srcs: {}".format(additional_compiled_srcs)
        )

    with TemporaryDirectory() as temp_dir:
        javac_output = None
        if not skip_javac_run:
            javac_output = _run_javac(
                javac_tool,
                javac_args,
                zipped_sources_file,
                javac_classpath,
                javac_processor_classpath,
                javac_bootclasspath_file,
                generated_sources_dir,
                temp_dir,
            )

        _run_jar(
            jar_builder_tool,
            output_path,
            manifest,
            javac_output,
            resources_dir,
            additional_compiled_srcs,
            remove_classes_file,
            temp_dir,
        )


if __name__ == "__main__":
    main()
