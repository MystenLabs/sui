# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.


import argparse
import os
import pathlib
import shutil
import zipfile
from tempfile import TemporaryDirectory
from typing import List

from java.tools import utils

_JAVA_OR_KOTLIN_FILE_EXTENSION = [".java", ".kt"]

_PLUGIN = "-P"
_X_PLUGIN_ARG = "-Xplugin"
_KAPT3_PLUGIN = "plugin:org.jetbrains.kotlin.kapt3:"
_APT_MODE_COMPILE = _KAPT3_PLUGIN + "aptMode=compile"
_AP_CLASSPATH_ARG = _KAPT3_PLUGIN + "apclasspath"
_AP_PROCESSORS_ARG = _KAPT3_PLUGIN + "processors"
_SOURCES_ARG = _KAPT3_PLUGIN + "sources"
_CLASSES_ARG = _KAPT3_PLUGIN + "classes"
_STUBS_ARG = _KAPT3_PLUGIN + "stubs"
_KAPT_GENERATED_ARG = "kapt.kotlin.generated"
_LIGHT_ANALYSIS = _KAPT3_PLUGIN + "useLightAnalysis"
_CORRECT_ERROR_TYPES = _KAPT3_PLUGIN + "correctErrorTypes"
_AP_OPTIONS = _KAPT3_PLUGIN + "apoptions"
_JAVAC_ARG = _KAPT3_PLUGIN + "javacArguments"

_KSP_PLUGIN = "plugin:com.google.devtools.ksp.symbol-processing:"
_KSP_AP_CLASSPATH_ARG = _KSP_PLUGIN + "apclasspath"
_KSP_PROJECT_BASE_DIR_ARG = _KSP_PLUGIN + "projectBaseDir"
_ksp_classes_and_resources_output_ARG = _KSP_PLUGIN + "classOutputDir"
_KSP_KOTLIN_OUTPUT_ARG = _KSP_PLUGIN + "kotlinOutputDir"
_KSP_JAVA_OUTPUT_ARG = _KSP_PLUGIN + "javaOutputDir"
_KSP_RESOURCE_OUTPUT_ARG = _KSP_PLUGIN + "resourceOutputDir"
_KSP_CACHES_DIR_ARG = _KSP_PLUGIN + "cachesDir"
_KSP_OUTPUT_ARG = _KSP_PLUGIN + "kspOutputDir"
_KSP_INCREMENTAL_ARG = _KSP_PLUGIN + "incremental"
_KSP_WITH_COMPILATION_ARG = _KSP_PLUGIN + "withCompilation"


def _parse_args():
    parser = argparse.ArgumentParser(description="Tool to compile kotlin source files.")

    parser.add_argument(
        "--kotlinc_cmd_file",
        type=pathlib.Path,
        required=False,
        metavar="kotlinc_cmd_file",
        help="path to file with the command that should be run for compilation",
    )
    parser.add_argument(
        "--kotlinc_output",
        type=pathlib.Path,
        required=False,
        metavar="kotlinc_output",
        help="path to .class files produced by running kotlinc",
    )
    parser.add_argument(
        "--zipped_sources_file",
        type=pathlib.Path,
        required=False,
        metavar="zipped_sources_file",
        help="path to file with stored zipped source files that need to be passed to kotlin compiler",
    )
    parser.add_argument(
        "--kapt_annotation_processing_jar",
        type=str,
        required=False,
        metavar="kapt_annotation_processing_jar",
        help="annotation processing jar used by KAPT",
    )
    parser.add_argument(
        "--kapt_annotation_processors",
        type=str,
        required=False,
        metavar="kapt_annotation_processors",
        help="comma separated list of annotation processors to be run by KAPT",
    )
    parser.add_argument(
        "--kapt_annotation_processor_params",
        type=str,
        required=False,
        metavar="kapt_annotation_processor_params",
        help="comma separated list of annotation processor params to be passed to KAPT",
    )
    parser.add_argument(
        "--kapt_classpath_file",
        type=pathlib.Path,
        required=False,
        metavar="kapt_classpath_file",
        help="path to file with the classpath that should be used for KAPT",
    )
    parser.add_argument(
        "--kapt_sources_output",
        type=pathlib.Path,
        required=False,
        metavar="kapt_sources_output",
        help="path where kapt-generated sources should be written",
    )
    parser.add_argument(
        "--kapt_generated_sources_output",
        type=pathlib.Path,
        required=False,
        metavar="kapt_generated_sources_output",
        help="path generated sources should be zipped",
    )
    parser.add_argument(
        "--kapt_classes_output",
        type=pathlib.Path,
        required=False,
        metavar="kapt_classes_output",
        help="path where kapt-generated classes should be written",
    )
    parser.add_argument(
        "--kapt_stubs",
        type=pathlib.Path,
        required=False,
        metavar="kapt_stubs",
        help="path where kapt-generated stubs should be written",
    )
    parser.add_argument(
        "--kapt_base64_encoder",
        type=str,
        required=False,
        metavar="kapt_base64_encoder",
        help="tool for doing base64 encoding for KAPT",
    )
    parser.add_argument(
        "--kapt_generated_kotlin_output",
        type=pathlib.Path,
        required=False,
        metavar="kapt_generated_kotlin_output",
        help="path where kapt-generated kotlin output should be written",
    )
    parser.add_argument(
        "--kapt_jvm_target",
        type=str,
        required=False,
        metavar="kapt_jvm_target",
        help="JVM target to use for KAPT call",
    )
    parser.add_argument(
        "--ksp_processor_jars",
        type=str,
        required=False,
        metavar="ksp_processor_jars",
        help="comma separated list of jars containing KSP annotation processors",
    )
    parser.add_argument(
        "--ksp_classpath",
        type=str,
        required=False,
        metavar="ksp_classpath",
        help="classpath to be passed to KSP (so that it can find the resources it needs)",
    )
    parser.add_argument(
        "--ksp_classes_and_resources_output",
        type=pathlib.Path,
        required=False,
        metavar="ksp_classes_and_resources_output",
        help="path where ksp-generated classes and resources should be written",
    )
    parser.add_argument(
        "--ksp_sources_output",
        type=pathlib.Path,
        required=False,
        metavar="ksp_sources_output",
        help="path where ksp-generated Java and Kotlin sources should be written",
    )
    parser.add_argument(
        "--ksp_zipped_sources_output",
        type=pathlib.Path,
        required=False,
        metavar="ksp_zipped_sources_output",
        help="path where zipped ksp-generated Java and Kotlin sources should be written",
    )
    parser.add_argument(
        "--ksp_output",
        type=pathlib.Path,
        required=False,
        metavar="ksp_output",
        help="root of KSP output dirs",
    )
    parser.add_argument(
        "--ksp_project_base_dir",
        type=pathlib.Path,
        required=False,
        metavar="ksp_project_base_dir",
        help="project dir for this KSP invocation",
    )
    parser.add_argument(
        "--ksp_generated_classes_and_resources",
        type=pathlib.Path,
        required=False,
        metavar="ksp_generated_classes_and_resources",
        help="classes and resources that were generated by a previous invocation of KSP",
    )
    parser.add_argument(
        "--kotlin_compiler_plugin_dir",
        type=pathlib.Path,
        required=False,
        metavar="kotlin_compiler_plugin_dir",
        help="Directory for KAPT compiler plugins to use",
    )
    parser.add_argument(
        "--zip_scrubber",
        required=True,
        help="tool for scrubbing timestamps from zip files to produce deterministic output",
    )
    return parser.parse_args()


def _run_kotlinc(
    kotlinc_output: pathlib.Path,
    kotlinc_cmd_file: pathlib.Path,
    zipped_sources_file: pathlib.Path,
    ksp_cmd: List[str],
    kapt_cmd: List[str],
    temp_dir: TemporaryDirectory,
) -> pathlib.Path:
    kotlinc_cmd = []

    cmd_file = kotlinc_cmd_file
    if zipped_sources_file:
        cmd_file = utils.extract_source_files(
            zipped_sources_file,
            kotlinc_cmd_file,
            _JAVA_OR_KOTLIN_FILE_EXTENSION,
            temp_dir,
        )

    if utils.sources_are_present(cmd_file, [".kt"]):
        with open(cmd_file, "r") as file:
            kotlinc_cmd += [line.strip() for line in file.readlines()]

        kotlinc_cmd += ksp_cmd
        kotlinc_cmd += kapt_cmd

        if kotlinc_output:
            kotlinc_cmd += ["-d", kotlinc_output]

        utils.execute_command(kotlinc_cmd)
    else:
        os.mkdir(kotlinc_output)

    return kotlinc_output


def _encode_kapt_ap_options(
    kapt_annotation_processor_params: str,
    kapt_base64_encoder_cmd: List[str],
    kapt_generated_kotlin_output: pathlib.Path,
    temp_dir: TemporaryDirectory,
) -> str:
    ap_options_file = os.path.join(temp_dir, "ap_options.txt")
    with open(ap_options_file, "w") as file:
        file.write(
            "{}={}".format(_KAPT_GENERATED_ARG, str(kapt_generated_kotlin_output))
        )
        if kapt_annotation_processor_params:
            for param in kapt_annotation_processor_params.split(";"):
                file.write("\n")
                file.write(param)

    encoded_ap_options_file = os.path.join(temp_dir, "encoded_ap_options.txt")
    return _encode_options(
        kapt_base64_encoder_cmd, ap_options_file, encoded_ap_options_file
    )


def _encode_javac_arguments(
    jvm_target: str,
    kapt_base64_encoder_cmd: List[str],
    temp_dir: TemporaryDirectory,
) -> str:
    javac_arguments_file = os.path.join(temp_dir, "javac_arguments.txt")
    with open(javac_arguments_file, "w") as file:
        file.write("-source={}\n-target={}".format(jvm_target, jvm_target))

    encoded_javac_arguments_file = os.path.join(temp_dir, "encoded_javac_arguments.txt")
    return _encode_options(
        kapt_base64_encoder_cmd, javac_arguments_file, encoded_javac_arguments_file
    )


def _encode_options(
    kapt_base64_encoder_cmd: List[str],
    options_file: pathlib.Path,
    encoded_options_file: pathlib.Path,
) -> str:
    cmd = kapt_base64_encoder_cmd + [options_file, encoded_options_file]

    utils.execute_command(cmd)

    with open(encoded_options_file, "r") as file:
        return file.read().strip()


def _get_kapt_cmd(
    kapt_annotation_processing_jar: pathlib.Path,
    kapt_annotation_processors: str,
    kapt_annotation_processor_params: str,
    kapt_classpath_file: pathlib.Path,
    kapt_sources_output: pathlib.Path,
    kapt_classes_output: pathlib.Path,
    kapt_stubs: pathlib.Path,
    kapt_base64_encoder: pathlib.Path,
    kapt_generated_kotlin_output: pathlib.Path,
    kapt_jvm_target: str,
    temp_dir: TemporaryDirectory,
) -> List[str]:
    if not kapt_annotation_processors:
        return []

    kapt_plugin_options = [_APT_MODE_COMPILE]
    kapt_plugin_options += [
        "=".join([_AP_PROCESSORS_ARG, ap])
        for ap in kapt_annotation_processors.split(",")
    ]
    with open(kapt_classpath_file, "r") as file:
        kapt_plugin_options += [
            "=".join([_AP_CLASSPATH_ARG, line.strip()]) for line in file.readlines()
        ]

    kapt_base64_encoder_cmd = utils.shlex_split(kapt_base64_encoder)
    kapt_plugin_options += [
        "=".join([_SOURCES_ARG, str(kapt_sources_output)]),
        "=".join([_CLASSES_ARG, str(kapt_classes_output)]),
        "=".join([_STUBS_ARG, str(kapt_stubs)]),
        "=".join([_LIGHT_ANALYSIS, "true"]),
        "=".join([_CORRECT_ERROR_TYPES, "true"]),
        "=".join(
            [
                _AP_OPTIONS,
                _encode_kapt_ap_options(
                    kapt_annotation_processor_params,
                    kapt_base64_encoder_cmd,
                    kapt_generated_kotlin_output,
                    temp_dir,
                ),
            ]
        ),
        "=".join(
            [
                _JAVAC_ARG,
                _encode_javac_arguments(
                    kapt_jvm_target, kapt_base64_encoder_cmd, temp_dir
                ),
            ]
        ),
    ]

    return [
        "=".join([_X_PLUGIN_ARG, str(kapt_annotation_processing_jar)]),
        _PLUGIN,
        ",".join(kapt_plugin_options),
    ]


def _get_ksp_cmd(
    ksp_processors_jars: str,
    ksp_classpath: str,
    ksp_project_base_dir: pathlib.Path,
    base_ksp_output_dir: pathlib.Path,
    ksp_classes_and_resources_output: pathlib.Path,
    ksp_sources_output: pathlib.Path,
) -> List[str]:
    if not ksp_processors_jars:
        return []

    ksp_plugin_options = [
        "{}={}".format(
            _KSP_AP_CLASSPATH_ARG, os.pathsep.join(ksp_processors_jars.split(","))
        ),
        "{}={}".format(_KSP_PLUGIN + "apoption=cp", ksp_classpath),
        "{}={}".format(_KSP_PROJECT_BASE_DIR_ARG, ksp_project_base_dir.resolve()),
        "{}={}".format(
            _ksp_classes_and_resources_output_ARG,
            ksp_classes_and_resources_output.resolve(),
        ),
        "{}={}".format(_KSP_KOTLIN_OUTPUT_ARG, ksp_sources_output.resolve()),
        "{}={}".format(_KSP_JAVA_OUTPUT_ARG, ksp_sources_output.resolve()),
        "{}={}".format(
            _KSP_RESOURCE_OUTPUT_ARG, ksp_classes_and_resources_output.resolve()
        ),
        "{}={}".format(_KSP_OUTPUT_ARG, base_ksp_output_dir.resolve()),
        "{}={}".format(_KSP_CACHES_DIR_ARG, base_ksp_output_dir.resolve()),
        "{}={}".format(_KSP_INCREMENTAL_ARG, False),
        "{}={}".format(_KSP_WITH_COMPILATION_ARG, False),
    ]

    return [
        _PLUGIN,
        ",".join(ksp_plugin_options),
    ]


def _zip_recursive(archive_path: pathlib.Path, source_path: pathlib.Path):
    # Same as 'zip -r archive_path source_path'
    with zipfile.ZipFile(
        archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6
    ) as z:
        z.write(source_path)
        for f in source_path.glob("**/*"):
            z.write(f)


def main():
    args = _parse_args()

    output_path = args.kotlinc_output
    kotlinc_cmd_file = args.kotlinc_cmd_file
    zipped_sources_file = args.zipped_sources_file
    kapt_annotation_processing_jar = args.kapt_annotation_processing_jar
    kapt_annotation_processors = args.kapt_annotation_processors
    kapt_annotation_processor_params = args.kapt_annotation_processor_params
    kapt_classpath_file = args.kapt_classpath_file
    kapt_sources_output = args.kapt_sources_output
    kapt_classes_output = args.kapt_classes_output
    kapt_generated_sources_output = args.kapt_generated_sources_output
    kapt_stubs = args.kapt_stubs
    kapt_base64_encoder = args.kapt_base64_encoder
    kapt_generated_kotlin_output = args.kapt_generated_kotlin_output
    kapt_jvm_target = args.kapt_jvm_target
    kotlin_compiler_plugin_dir = args.kotlin_compiler_plugin_dir
    ksp_processor_jars = args.ksp_processor_jars
    ksp_classpath = args.ksp_classpath
    ksp_output = args.ksp_output
    ksp_project_base_dir = args.ksp_project_base_dir
    ksp_classes_and_resources_output = args.ksp_classes_and_resources_output
    ksp_sources_output = args.ksp_sources_output
    ksp_zipped_sources_output = args.ksp_zipped_sources_output
    ksp_generated_classes_and_resources = args.ksp_generated_classes_and_resources
    zip_scrubber = args.zip_scrubber

    utils.log_message("output: {}".format(output_path))
    utils.log_message("kotlinc_cmd_file: {}".format(kotlinc_cmd_file))
    if zipped_sources_file:
        utils.log_message("zipped_sources_file: {}".format(zipped_sources_file))
    utils.log_message(
        "kapt_annotation_processing_jar: {}".format(kapt_annotation_processing_jar)
    )
    utils.log_message(
        "kapt_annotation_processors: {}".format(kapt_annotation_processors)
    )
    utils.log_message(
        "kapt_annotation_processor_params: {}".format(kapt_annotation_processor_params)
    )
    utils.log_message("kapt_classpath_file: {}".format(kapt_classpath_file))
    utils.log_message("kapt_sources_output: {}".format(kapt_sources_output))
    utils.log_message(
        "kapt_generated_sources_output: {}".format(kapt_generated_sources_output)
    )
    utils.log_message("kapt_classes_output: {}".format(kapt_classes_output))
    utils.log_message("kapt_stubs: {}".format(kapt_stubs))
    utils.log_message("kapt_base64_encoder: {}".format(kapt_base64_encoder))
    utils.log_message(
        "kapt_generated_kotlin_output: {}".format(kapt_generated_kotlin_output)
    )
    utils.log_message("kapt_jvm_target: {}".format(kapt_jvm_target))
    utils.log_message(
        "kotlin_compiler_plugin_dir: {}".format(kotlin_compiler_plugin_dir)
    )
    utils.log_message("ksp_processor_jars: {}".format(ksp_processor_jars))
    utils.log_message("ksp_classpath: {}".format(ksp_classpath))
    utils.log_message("ksp_output: {}".format(ksp_output))
    utils.log_message("ksp_project_base_dir: {}".format(ksp_project_base_dir))
    utils.log_message(
        "ksp_classes_and_resources_output: {}".format(ksp_classes_and_resources_output)
    )
    utils.log_message("ksp_sources_output: {}".format(ksp_sources_output))
    utils.log_message("ksp_zipped_sources_output: {}".format(ksp_zipped_sources_output))
    utils.log_message(
        "ksp_generated_classes_and_resources: {}".format(
            ksp_generated_classes_and_resources
        )
    )
    utils.log_message("zip_scrubber: {}".format(zip_scrubber))

    if (
        kapt_annotation_processing_jar
        or kapt_annotation_processors
        or kapt_classpath_file
        or kapt_sources_output
        or kapt_classes_output
        or kapt_generated_sources_output
        or kapt_stubs
        or kapt_base64_encoder
        or kapt_generated_kotlin_output
        or kapt_jvm_target
    ):
        assert (
            kapt_annotation_processing_jar
            and kapt_annotation_processors
            and kapt_classpath_file
            and kapt_sources_output
            and kapt_classes_output
            and kapt_generated_sources_output
            and kapt_stubs
            and kapt_base64_encoder
            and kapt_generated_kotlin_output
        )
    if (
        ksp_processor_jars
        or ksp_classpath
        or ksp_output
        or ksp_project_base_dir
        or ksp_classes_and_resources_output
        or ksp_sources_output
        or ksp_zipped_sources_output
    ):
        assert (
            ksp_processor_jars
            and ksp_classpath
            and ksp_output
            and ksp_project_base_dir
            and ksp_classes_and_resources_output
            and ksp_sources_output
            and ksp_zipped_sources_output
        )

    with TemporaryDirectory() as temp_dir:
        ksp_cmd = _get_ksp_cmd(
            ksp_processor_jars,
            ksp_classpath,
            ksp_project_base_dir,
            ksp_output,
            ksp_classes_and_resources_output,
            ksp_sources_output,
        )
        kapt_cmd = _get_kapt_cmd(
            kapt_annotation_processing_jar,
            kapt_annotation_processors,
            kapt_annotation_processor_params,
            kapt_classpath_file,
            kapt_sources_output,
            kapt_classes_output,
            kapt_stubs,
            kapt_base64_encoder,
            kapt_generated_kotlin_output,
            kapt_jvm_target,
            temp_dir,
        )
        _run_kotlinc(
            output_path,
            kotlinc_cmd_file,
            zipped_sources_file,
            ksp_cmd,
            kapt_cmd,
            temp_dir,
        )

    if ksp_sources_output:
        if not ksp_sources_output.exists():
            ksp_sources_output.mkdir()
        _zip_recursive(ksp_zipped_sources_output, ksp_sources_output)
        utils.execute_command(
            utils.shlex_split(zip_scrubber) + [ksp_zipped_sources_output]
        )

    if ksp_classes_and_resources_output:
        if not ksp_classes_and_resources_output.exists():
            ksp_classes_and_resources_output.mkdir()
    if kapt_sources_output:
        if not os.path.exists(kapt_sources_output):
            os.mkdir(kapt_sources_output)
            kapt_generated_sources_output.touch()
        else:
            _zip_recursive(kapt_generated_sources_output, kapt_sources_output)
            utils.execute_command(
                utils.shlex_split(zip_scrubber) + [kapt_generated_sources_output]
            )

    if kapt_classes_output:
        if not os.path.exists(kapt_classes_output):
            os.mkdir(kapt_classes_output)
        else:
            shutil.copytree(kapt_classes_output, output_path, dirs_exist_ok=True)
    if kapt_stubs and not os.path.exists(kapt_stubs):
        os.mkdir(kapt_stubs)
    if kapt_generated_kotlin_output and not os.path.exists(
        kapt_generated_kotlin_output
    ):
        os.mkdir(kapt_generated_kotlin_output)
    if kotlin_compiler_plugin_dir and not kotlin_compiler_plugin_dir.exists():
        kotlin_compiler_plugin_dir.mkdir()
    if ksp_generated_classes_and_resources:
        shutil.copytree(
            ksp_generated_classes_and_resources, output_path, dirs_exist_ok=True
        )


if __name__ == "__main__":
    main()
