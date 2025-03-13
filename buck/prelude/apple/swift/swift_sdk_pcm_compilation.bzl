# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//apple:apple_toolchain_types.bzl", "AppleToolchainInfo")
load("@prelude//apple:apple_utility.bzl", "expand_relative_prefixed_sdk_path", "get_disable_pch_validation_flags")
load(":apple_sdk_modules_utility.bzl", "get_compiled_sdk_clang_deps_tset")
load(
    ":swift_debug_info_utils.bzl",
    "extract_and_merge_clang_debug_infos",
)
load(":swift_toolchain_types.bzl", "SdkUncompiledModuleInfo", "SwiftCompiledModuleInfo", "SwiftCompiledModuleTset", "WrappedSdkCompiledModuleInfo")

def get_shared_pcm_compilation_args(module_name: str) -> cmd_args:
    cmd = cmd_args()
    cmd.add([
        "-emit-pcm",
        "-module-name",
        module_name,
        "-Xfrontend",
        "-disable-implicit-swift-modules",
        "-Xcc",
        "-fno-implicit-modules",
        "-Xcc",
        "-fno-implicit-module-maps",
        # Embed all input files into the PCM so we don't need to include module map files when
        # building remotely.
        # https://github.com/apple/llvm-project/commit/fb1e7f7d1aca7bcfc341e9214bda8b554f5ae9b6
        "-Xcc",
        "-Xclang",
        "-Xcc",
        "-fmodules-embed-all-files",
        # Set the base directory of the pcm file to the working directory, which ensures
        # all paths serialized in the PCM are relative.
        "-Xcc",
        "-Xclang",
        "-Xcc",
        "-fmodule-file-home-is-cwd",
        # We cannot set an empty Swift working directory as that would end up serializing
        # absolute header search paths in the PCM. Instead unset the clang working directory
        # to avoid serializing it as an absolute path.
        "-Xcc",
        "-working-directory=",
        # Using a relative resource dir requires we add the working directory as a search
        # path to be able to find the compiler generated includes.
        "-Xcc",
        "-I.",
    ])

    cmd.add(get_disable_pch_validation_flags())

    return cmd

def _remove_path_components_from_right(path: str, count: int):
    path_components = path.split("/")
    removed_path = "/".join(path_components[0:-count])
    return removed_path

def _add_sdk_module_search_path(cmd, uncompiled_sdk_module_info, apple_toolchain):
    modulemap_path = uncompiled_sdk_module_info.input_relative_path

    # If this input is a framework we need to search above the
    # current framework location, otherwise we include the
    # modulemap root.
    if uncompiled_sdk_module_info.is_framework:
        frameworks_dir_path = _remove_path_components_from_right(modulemap_path, 3)
        expanded_path = expand_relative_prefixed_sdk_path(
            cmd_args(apple_toolchain.swift_toolchain_info.sdk_path),
            cmd_args(apple_toolchain.swift_toolchain_info.resource_dir),
            cmd_args(apple_toolchain.platform_path),
            frameworks_dir_path,
        )
    else:
        module_root_path = _remove_path_components_from_right(modulemap_path, 1)
        expanded_path = expand_relative_prefixed_sdk_path(
            cmd_args(apple_toolchain.swift_toolchain_info.sdk_path),
            cmd_args(apple_toolchain.swift_toolchain_info.resource_dir),
            cmd_args(apple_toolchain.platform_path),
            module_root_path,
        )
    cmd.add([
        "-Xcc",
        ("-F" if uncompiled_sdk_module_info.is_framework else "-I"),
        "-Xcc",
        cmd_args(expanded_path),
    ])

def get_swift_sdk_pcm_anon_targets(
        ctx: AnalysisContext,
        uncompiled_sdk_deps: list[Dependency],
        swift_cxx_args: list[str]):
    # We include the Swift deps here too as we need
    # to include their transitive clang deps.
    return [
        (_swift_sdk_pcm_compilation, {
            "dep": module_dep,
            "name": module_dep.label,
            "swift_cxx_args": swift_cxx_args,
            "_apple_toolchain": ctx.attrs._apple_toolchain,
        })
        for module_dep in uncompiled_sdk_deps
    ]

def _swift_sdk_pcm_compilation_impl(ctx: AnalysisContext) -> [Promise, list[Provider]]:
    def k(sdk_pcm_deps_providers) -> list[Provider]:
        uncompiled_sdk_module_info = ctx.attrs.dep[SdkUncompiledModuleInfo]
        sdk_deps_tset = get_compiled_sdk_clang_deps_tset(ctx, sdk_pcm_deps_providers)

        # We pass in Swift and Clang SDK module deps to get the transitive
        # Clang dependencies compiled with the correct Swift cxx args. For
        # Swift modules we just want to pass up the clang deps.
        if uncompiled_sdk_module_info.is_swiftmodule:
            return [
                DefaultInfo(),
                WrappedSdkCompiledModuleInfo(
                    clang_deps = sdk_deps_tset,
                    clang_debug_info = extract_and_merge_clang_debug_infos(ctx, sdk_pcm_deps_providers),
                ),
            ]

        module_name = uncompiled_sdk_module_info.module_name
        apple_toolchain = ctx.attrs._apple_toolchain[AppleToolchainInfo]
        swift_toolchain = apple_toolchain.swift_toolchain_info
        cmd = cmd_args(swift_toolchain.compiler)
        cmd.add(uncompiled_sdk_module_info.partial_cmd)
        cmd.add(["-sdk", swift_toolchain.sdk_path])
        cmd.add(swift_toolchain.compiler_flags)

        if swift_toolchain.resource_dir:
            cmd.add([
                "-resource-dir",
                swift_toolchain.resource_dir,
            ])

        cmd.add(sdk_deps_tset.project_as_args("clang_deps"))

        expanded_modulemap_path_cmd = expand_relative_prefixed_sdk_path(
            cmd_args(swift_toolchain.sdk_path),
            cmd_args(swift_toolchain.resource_dir),
            cmd_args(apple_toolchain.platform_path),
            uncompiled_sdk_module_info.input_relative_path,
        )
        pcm_output = ctx.actions.declare_output(module_name + ".pcm")
        cmd.add([
            "-o",
            pcm_output.as_output(),
            expanded_modulemap_path_cmd,
        ])

        # For SDK modules we need to set a few more args
        cmd.add([
            "-Xcc",
            "-Xclang",
            "-Xcc",
            "-emit-module",
            "-Xcc",
            "-Xclang",
            "-Xcc",
            "-fsystem-module",
        ])

        cmd.add(ctx.attrs.swift_cxx_args)

        _add_sdk_module_search_path(cmd, uncompiled_sdk_module_info, apple_toolchain)

        ctx.actions.run(
            cmd,
            category = "sdk_swift_pcm_compile",
            identifier = module_name,
            # Swift compiler requires unique inodes for all input files.
            unique_input_inodes = True,
        )

        # Construct the args needed to be passed to the clang importer
        clang_importer_args = cmd_args()
        clang_importer_args.add("-Xcc")
        clang_importer_args.add(
            cmd_args(
                [
                    "-fmodule-file=",
                    module_name,
                    "=",
                    pcm_output,
                ],
                delimiter = "",
            ),
        )
        clang_importer_args.add("-Xcc")
        clang_importer_args.add(
            cmd_args(
                [
                    "-fmodule-map-file=",
                    expanded_modulemap_path_cmd,
                ],
                delimiter = "",
            ),
        )

        compiled_sdk = SwiftCompiledModuleInfo(
            clang_importer_args = clang_importer_args,
            is_framework = uncompiled_sdk_module_info.is_framework,
            is_swiftmodule = False,
            module_name = module_name,
            output_artifact = pcm_output,
        )

        return [
            DefaultInfo(),
            WrappedSdkCompiledModuleInfo(
                clang_deps = ctx.actions.tset(SwiftCompiledModuleTset, value = compiled_sdk, children = [sdk_deps_tset]),
                clang_debug_info = extract_and_merge_clang_debug_infos(ctx, sdk_pcm_deps_providers, [pcm_output]),
            ),
        ]

    # Compile the transitive clang module deps of this target.
    clang_module_deps = get_swift_sdk_pcm_anon_targets(
        ctx,
        ctx.attrs.dep[SdkUncompiledModuleInfo].deps,
        ctx.attrs.swift_cxx_args,
    )

    return ctx.actions.anon_targets(clang_module_deps).promise.map(k)

_swift_sdk_pcm_compilation = rule(
    impl = _swift_sdk_pcm_compilation_impl,
    attrs = {
        "dep": attrs.dep(),
        "swift_cxx_args": attrs.list(attrs.string(), default = []),
        "_apple_toolchain": attrs.dep(),
    },
)
