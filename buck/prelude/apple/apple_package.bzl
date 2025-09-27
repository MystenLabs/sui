# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//:paths.bzl", "paths")
load("@prelude//utils:arglike.bzl", "ArgLike")
load(":apple_bundle_destination.bzl", "AppleBundleDestination", "bundle_relative_path_for_destination")
load(":apple_bundle_types.bzl", "AppleBundleInfo")
load(":apple_package_config.bzl", "IpaCompressionLevel")
load(":apple_sdk.bzl", "get_apple_sdk_name")
load(":apple_swift_stdlib.bzl", "should_copy_swift_stdlib")
load(":apple_toolchain_types.bzl", "AppleToolchainInfo", "AppleToolsInfo")

def apple_package_impl(ctx: AnalysisContext) -> list[Provider]:
    package = ctx.actions.declare_output("{}.{}".format(ctx.attrs.bundle.label.name, ctx.attrs.ext))

    if ctx.attrs.packager:
        process_ipa_cmd = cmd_args([
            ctx.attrs.packager[RunInfo],
            "--app-bundle-path",
            ctx.attrs.bundle[DefaultInfo].default_outputs[0],
            "--output-path",
            package.as_output(),
            ctx.attrs.packager_args,
        ])
        category = "apple_package_make_custom"

        if ctx.attrs.validator:
            fail(
                "{} doesn't support a setting `packager` and `validator` at the same time.".format(ctx.attrs.name),
            )

    else:
        unprocessed_ipa_contents = _get_ipa_contents(ctx)
        process_ipa_cmd = _get_default_package_cmd(
            ctx,
            unprocessed_ipa_contents,
            package.as_output(),
        )
        category = "apple_package_make"

    ctx.actions.run(process_ipa_cmd, category = category)

    return [DefaultInfo(default_output = package)]

def _get_default_package_cmd(ctx: AnalysisContext, unprocessed_ipa_contents: Artifact, output: OutputArtifact) -> cmd_args:
    apple_tools = ctx.attrs._apple_tools[AppleToolsInfo]
    process_ipa_cmd = cmd_args([
        apple_tools.ipa_package_maker,
        "--ipa-contents-dir",
        unprocessed_ipa_contents,
        "--ipa-output-path",
        output,
        "--compression-level",
        _compression_level_arg(IpaCompressionLevel(ctx.attrs._ipa_compression_level)),
    ])
    if ctx.attrs.validator != None:
        process_ipa_cmd.add([
            "--validator",
            ctx.attrs.validator[RunInfo],
        ])

    return process_ipa_cmd

def _get_ipa_contents(ctx: AnalysisContext) -> Artifact:
    bundle = ctx.attrs.bundle
    app = bundle[DefaultInfo].default_outputs[0]

    contents = {
        paths.join("Payload", app.basename): app,
    }

    apple_bundle_info = bundle[AppleBundleInfo]
    if (not apple_bundle_info.skip_copying_swift_stdlib) and should_copy_swift_stdlib(app.extension):
        swift_support_path = paths.join("SwiftSupport", get_apple_sdk_name(ctx))
        contents[swift_support_path] = _get_swift_support_dir(ctx, app, apple_bundle_info)

    if apple_bundle_info.contains_watchapp:
        contents["Symbols"] = _build_symbols_dir(ctx)

    return ctx.actions.copied_dir(
        "__unzipped_ipa_contents__",
        contents,
    )

def _build_symbols_dir(ctx) -> Artifact:
    symbols_dir = ctx.actions.declare_output("__symbols__", dir = True)
    ctx.actions.run(
        cmd_args(["mkdir", "-p", symbols_dir.as_output()]),
        category = "watchos_symbols_dir",
    )

    return symbols_dir

def _get_swift_support_dir(ctx, bundle_output: Artifact, bundle_info: AppleBundleInfo) -> Artifact:
    stdlib_tool = ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info.swift_stdlib_tool
    sdk_name = get_apple_sdk_name(ctx)

    # .app -> app
    # This is the way the input is expected.
    extension = bundle_output.extension[1:]
    swift_support_dir = ctx.actions.declare_output("__swift_dylibs__", dir = True)
    script, _ = ctx.actions.write(
        "build_swift_support.sh",
        [
            cmd_args("set -euo pipefail"),
            cmd_args(swift_support_dir, format = "mkdir -p {}"),
            cmd_args(
                [
                    stdlib_tool,
                    # If you're debugging, you can pass the '--verbose' flag here.
                    "--copy",
                    "--scan-executable",
                    cmd_args(
                        [
                            bundle_output,
                            bundle_relative_path_for_destination(AppleBundleDestination("executables"), sdk_name, extension),
                            bundle_info.binary_name,
                        ],
                        delimiter = "/",
                    ),
                    _get_scan_folder_args(AppleBundleDestination("plugins"), bundle_output, sdk_name, extension),
                    _get_scan_folder_args(AppleBundleDestination("frameworks"), bundle_output, sdk_name, extension),
                    _get_scan_folder_args(AppleBundleDestination("appclips"), bundle_output, sdk_name, extension),
                    "--destination",
                    swift_support_dir,
                ],
                delimiter = " ",
                quote = "shell",
            ),
        ],
        allow_args = True,
    )
    ctx.actions.run(
        cmd_args(["/bin/sh", script]).hidden([stdlib_tool, bundle_output, swift_support_dir.as_output()]),
        category = "copy_swift_stdlibs",
    )

    return swift_support_dir

def _get_scan_folder_args(dest: AppleBundleDestination, bundle_output: Artifact, sdk_name, extension) -> ArgLike:
    return cmd_args(
        [
            "--scan-folder",
            cmd_args(
                [
                    bundle_output,
                    bundle_relative_path_for_destination(dest, sdk_name, extension),
                ],
                delimiter = "/",
            ),
        ],
    )

def _compression_level_arg(compression_level: IpaCompressionLevel) -> str:
    if compression_level.value == "none":
        return "0"
    elif compression_level.value == "default":
        return "6"
    elif compression_level.value == "min":
        return "1"
    elif compression_level.value == "max":
        return "9"
    else:
        fail("Unknown .ipa compression level: " + str(compression_level))
