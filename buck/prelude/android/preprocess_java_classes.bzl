# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//android:android_toolchain.bzl", "AndroidToolchainInfo")
load("@prelude//java:java_toolchain.bzl", "JavaToolchainInfo")
load("@prelude//java/utils:java_more_utils.bzl", "get_path_separator_for_exec_os")
load("@prelude//utils:utils.bzl", "expect")

def get_preprocessed_java_classes(ctx: AnalysisContext, input_jars = {"artifact": "target_label"}) -> dict[Artifact, TargetLabel]:
    if not input_jars:
        return {}

    input_srcs = {}
    output_jars_to_owners = {}
    output_dir = ctx.actions.declare_output("preprocessed_java_classes/output_dir")

    for i, (input_jar, target_label) in enumerate(input_jars.items()):
        expect(input_jar.extension == ".jar", "Expected {} to have extension .jar!".format(input_jar))
        jar_name = "{}_{}".format(i, input_jar.basename)
        input_srcs[jar_name] = input_jar
        output_jar = output_dir.project(jar_name)
        output_jars_to_owners[output_jar] = target_label

    input_dir = ctx.actions.symlinked_dir("preprocessed_java_classes/input_dir", input_srcs)

    env = {
        "ANDROID_BOOTCLASSPATH": cmd_args(
            ctx.attrs._android_toolchain[AndroidToolchainInfo].android_bootclasspath,
            delimiter = get_path_separator_for_exec_os(ctx),
        ),
        "IN_JARS_DIR": cmd_args(input_dir),
        "OUT_JARS_DIR": output_dir.as_output(),
        "PREPROCESS": ctx.attrs.preprocess_java_classes_bash,
        "ZIP_SCRUBBER": ctx.attrs._java_toolchain[JavaToolchainInfo].zip_scrubber,
    }

    output_jars = output_jars_to_owners.keys()
    output_jars_file = ctx.actions.write("preprocessed_java_classes/output_jars.txt", output_jars)

    preprocess_cmd = [
        "/usr/bin/env",
        "bash",
        "-c",
        # Note: ZIP_SCRUBBER might expand to multiple words, so no quoting there.
        'mkdir -p "$OUT_JARS_DIR" && eval "$PREPROCESS" && $ZIP_SCRUBBER --paths-to-scrub "$@"',
        "--",
        output_jars_file,
    ]

    preprocess_cmd = cmd_args(preprocess_cmd)
    preprocess_cmd.hidden([output_jar.as_output() for output_jar in output_jars])
    for dep in ctx.attrs.preprocess_java_classes_deps:
        preprocess_cmd.hidden(dep[DefaultInfo].default_outputs + dep[DefaultInfo].other_outputs)

    ctx.actions.run(preprocess_cmd, env = env, category = "preprocess_java_classes")

    return output_jars_to_owners
