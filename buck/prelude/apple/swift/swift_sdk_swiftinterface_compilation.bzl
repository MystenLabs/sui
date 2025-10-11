# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//apple:apple_toolchain_types.bzl", "AppleToolchainInfo")
load("@prelude//apple:apple_utility.bzl", "expand_relative_prefixed_sdk_path")
load("@prelude//apple/swift:swift_types.bzl", "SWIFTMODULE_EXTENSION")
load(":apple_sdk_modules_utility.bzl", "get_compiled_sdk_clang_deps_tset", "get_compiled_sdk_swift_deps_tset")
load(
    ":swift_debug_info_utils.bzl",
    "extract_and_merge_clang_debug_infos",
    "extract_and_merge_swift_debug_infos",
)
load(":swift_module_map.bzl", "write_swift_module_map")
load(":swift_sdk_pcm_compilation.bzl", "get_swift_sdk_pcm_anon_targets")
load(":swift_toolchain_types.bzl", "SdkUncompiledModuleInfo", "SwiftCompiledModuleInfo", "SwiftCompiledModuleTset", "WrappedSdkCompiledModuleInfo")

def get_swift_interface_anon_targets(
        ctx: AnalysisContext,
        uncompiled_sdk_deps: list[Dependency]):
    return [
        (
            _swift_interface_compilation,
            {
                "dep": d,
                "name": d.label,
                "_apple_toolchain": ctx.attrs._apple_toolchain,
            },
        )
        for d in uncompiled_sdk_deps
        if d[SdkUncompiledModuleInfo].is_swiftmodule
    ]

def _swift_interface_compilation_impl(ctx: AnalysisContext) -> [Promise, list[Provider]]:
    def k(sdk_deps_providers) -> list[Provider]:
        uncompiled_sdk_module_info = ctx.attrs.dep[SdkUncompiledModuleInfo]
        uncompiled_module_info_name = uncompiled_sdk_module_info.module_name
        apple_toolchain = ctx.attrs._apple_toolchain[AppleToolchainInfo]
        swift_toolchain = apple_toolchain.swift_toolchain_info
        cmd = cmd_args(swift_toolchain.compiler)
        cmd.add(uncompiled_sdk_module_info.partial_cmd)
        cmd.add(["-sdk", swift_toolchain.sdk_path])

        if swift_toolchain.resource_dir:
            cmd.add([
                "-resource-dir",
                swift_toolchain.resource_dir,
            ])

        clang_deps_tset = get_compiled_sdk_clang_deps_tset(ctx, sdk_deps_providers)
        swift_deps_tset = get_compiled_sdk_swift_deps_tset(ctx, sdk_deps_providers)
        swift_module_map_artifact = write_swift_module_map(ctx, uncompiled_module_info_name, swift_deps_tset)
        cmd.add([
            "-explicit-swift-module-map-file",
            swift_module_map_artifact,
        ])
        cmd.add(clang_deps_tset.project_as_args("clang_deps"))

        swiftmodule_output = ctx.actions.declare_output(uncompiled_module_info_name + SWIFTMODULE_EXTENSION)
        expanded_swiftinterface_cmd = expand_relative_prefixed_sdk_path(
            cmd_args(swift_toolchain.sdk_path),
            cmd_args(swift_toolchain.resource_dir),
            cmd_args(apple_toolchain.platform_path),
            uncompiled_sdk_module_info.input_relative_path,
        )
        cmd.add([
            "-o",
            swiftmodule_output.as_output(),
            expanded_swiftinterface_cmd,
        ])

        ctx.actions.run(
            cmd,
            category = "sdk_swiftinterface_compile",
            identifier = uncompiled_module_info_name,
        )

        compiled_sdk = SwiftCompiledModuleInfo(
            is_framework = uncompiled_sdk_module_info.is_framework,
            is_swiftmodule = True,
            module_name = uncompiled_module_info_name,
            output_artifact = swiftmodule_output,
        )

        return [
            DefaultInfo(),
            WrappedSdkCompiledModuleInfo(
                swift_deps = ctx.actions.tset(SwiftCompiledModuleTset, value = compiled_sdk, children = [swift_deps_tset]),
                swift_debug_info = extract_and_merge_swift_debug_infos(ctx, sdk_deps_providers, [swiftmodule_output]),
                clang_debug_info = extract_and_merge_clang_debug_infos(ctx, sdk_deps_providers),
            ),
        ]

    # For each swiftinterface compile its transitive clang deps with the provided target.
    module_info = ctx.attrs.dep[SdkUncompiledModuleInfo]
    clang_module_deps = get_swift_sdk_pcm_anon_targets(
        ctx,
        module_info.deps,
        ["-target", module_info.target],
    )

    # Compile the transitive swiftmodule deps.
    swift_module_deps = get_swift_interface_anon_targets(ctx, module_info.deps)

    return ctx.actions.anon_targets(clang_module_deps + swift_module_deps).promise.map(k)

_swift_interface_compilation = rule(
    impl = _swift_interface_compilation_impl,
    attrs = {
        "dep": attrs.dep(),
        "_apple_toolchain": attrs.dep(),
    },
)
