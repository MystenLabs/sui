# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//apple:apple_toolchain_types.bzl", "AppleToolchainInfo")
load("@prelude//apple:apple_utility.bzl", "get_module_name")
load("@prelude//cxx:preprocessor.bzl", "cxx_inherited_preprocessor_infos", "cxx_merge_cpreprocessors")
load(
    ":apple_sdk_modules_utility.bzl",
    "get_compiled_sdk_clang_deps_tset",
    "get_uncompiled_sdk_deps",
)
load(
    ":swift_debug_info_utils.bzl",
    "extract_and_merge_clang_debug_infos",
)
load(":swift_pcm_compilation_types.bzl", "SwiftPCMUncompiledInfo", "WrappedSwiftPCMCompiledInfo")
load(":swift_sdk_pcm_compilation.bzl", "get_shared_pcm_compilation_args", "get_swift_sdk_pcm_anon_targets")
load(":swift_toolchain_types.bzl", "SwiftCompiledModuleInfo", "SwiftCompiledModuleTset", "WrappedSdkCompiledModuleInfo")

_REQUIRED_SDK_MODULES = ["Foundation"]

def get_compiled_pcm_deps_tset(ctx: AnalysisContext, pcm_deps_providers: list) -> SwiftCompiledModuleTset:
    pcm_deps = [
        pcm_deps_provider[WrappedSwiftPCMCompiledInfo].tset
        for pcm_deps_provider in pcm_deps_providers
        if WrappedSwiftPCMCompiledInfo in pcm_deps_provider
    ]
    return ctx.actions.tset(SwiftCompiledModuleTset, children = pcm_deps)

def get_swift_pcm_anon_targets(
        ctx: AnalysisContext,
        uncompiled_deps: list[Dependency],
        swift_cxx_args: list[str]):
    deps = [
        {
            "dep": uncompiled_dep,
            "name": uncompiled_dep.label,
            "swift_cxx_args": swift_cxx_args,
            "_apple_toolchain": ctx.attrs._apple_toolchain,
        }
        for uncompiled_dep in uncompiled_deps
        if SwiftPCMUncompiledInfo in uncompiled_dep
    ]
    return [(_swift_pcm_compilation, d) for d in deps]

def _compile_with_argsfile(
        ctx: AnalysisContext,
        category: str,
        module_name: str,
        args: cmd_args,
        additional_cmd: cmd_args):
    shell_quoted_cmd = cmd_args(args, quote = "shell")
    argfile, _ = ctx.actions.write(module_name + ".pcm.argsfile", shell_quoted_cmd, allow_args = True)

    swift_toolchain = ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info
    cmd = cmd_args(swift_toolchain.compiler)
    cmd.add(cmd_args(["@", argfile], delimiter = ""))

    # Action should also depend on all artifacts from the argsfile, otherwise they won't be materialised.
    cmd.hidden([args])

    cmd.add(additional_cmd)

    ctx.actions.run(
        cmd,
        category = category,
        identifier = module_name,
        # Swift compiler requires unique inodes for all input files.
        unique_input_inodes = True,
    )

def _compiled_module_info(
        module_name: str,
        pcm_output: Artifact,
        pcm_info: SwiftPCMUncompiledInfo) -> SwiftCompiledModuleInfo:
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
                pcm_info.exported_preprocessor.modulemap_path,
            ],
            delimiter = "",
        ),
    )
    clang_importer_args.add("-Xcc")
    clang_importer_args.add(pcm_info.exported_preprocessor.relative_args.args)
    clang_importer_args.hidden(pcm_info.exported_preprocessor.modular_args)

    return SwiftCompiledModuleInfo(
        clang_importer_args = clang_importer_args,
        is_framework = False,
        is_swiftmodule = False,
        module_name = module_name,
        output_artifact = pcm_output,
    )

def _swift_pcm_compilation_impl(ctx: AnalysisContext) -> [Promise, list[Provider]]:
    def k(compiled_pcm_deps_providers) -> list[Provider]:
        uncompiled_pcm_info = ctx.attrs.dep[SwiftPCMUncompiledInfo]

        # `compiled_pcm_deps_providers` will contain `WrappedSdkCompiledModuleInfo` providers
        # from direct SDK deps and transitive deps that export sdk deps.
        sdk_deps_tset = get_compiled_sdk_clang_deps_tset(ctx, compiled_pcm_deps_providers)

        # To compile a pcm we only use the exported_deps as those are the only
        # ones that should be transitively exported through public headers
        pcm_deps_tset = get_compiled_pcm_deps_tset(ctx, compiled_pcm_deps_providers)

        # We don't need to compile non-modular or targets that do not export any headers,
        # but for the sake of BUCK1 compatibility, we need to pass them up,
        # in case they re-export some dependencies.
        if uncompiled_pcm_info.is_transient:
            return [
                DefaultInfo(),
                WrappedSwiftPCMCompiledInfo(
                    tset = pcm_deps_tset,
                ),
                WrappedSdkCompiledModuleInfo(
                    clang_deps = sdk_deps_tset,
                    clang_debug_info = extract_and_merge_clang_debug_infos(ctx, compiled_pcm_deps_providers),
                ),
            ]

        module_name = uncompiled_pcm_info.name
        cmd, additional_cmd, pcm_output = _get_base_pcm_flags(
            ctx,
            module_name,
            uncompiled_pcm_info,
            sdk_deps_tset,
            pcm_deps_tset,
            ctx.attrs.swift_cxx_args,
        )

        # It's possible that modular targets can re-export headers of non-modular targets,
        # (e.g `raw_headers`) because of that we need to provide search paths of such targets to
        # pcm compilation actions in order for them to be successful.
        inherited_preprocessor_infos = cxx_inherited_preprocessor_infos(uncompiled_pcm_info.exported_deps)
        preprocessors = cxx_merge_cpreprocessors(ctx, [], inherited_preprocessor_infos)
        cmd.add(cmd_args(preprocessors.set.project_as_args("include_dirs"), prepend = "-Xcc"))

        # When compiling pcm files, module's exported pps and inherited pps
        # must be provided to an action like hmaps which are used for headers resolution.
        if uncompiled_pcm_info.propagated_preprocessor_args_cmd:
            cmd.add(uncompiled_pcm_info.propagated_preprocessor_args_cmd)

        _compile_with_argsfile(
            ctx,
            "swift_pcm_compile",
            module_name,
            cmd,
            additional_cmd,
        )
        pcm_info = _compiled_module_info(module_name, pcm_output, uncompiled_pcm_info)

        return [
            DefaultInfo(default_outputs = [pcm_output]),
            WrappedSwiftPCMCompiledInfo(
                tset = ctx.actions.tset(SwiftCompiledModuleTset, value = pcm_info, children = [pcm_deps_tset]),
            ),
            WrappedSdkCompiledModuleInfo(
                clang_deps = sdk_deps_tset,
                clang_debug_info = extract_and_merge_clang_debug_infos(ctx, compiled_pcm_deps_providers, [pcm_info.output_artifact]),
            ),
        ]

    direct_uncompiled_sdk_deps = get_uncompiled_sdk_deps(
        ctx.attrs.dep[SwiftPCMUncompiledInfo].uncompiled_sdk_modules,
        _REQUIRED_SDK_MODULES,
        ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info,
    )

    # Recursively compiling SDK's Clang dependencies
    sdk_pcm_deps_anon_targets = get_swift_sdk_pcm_anon_targets(
        ctx,
        direct_uncompiled_sdk_deps,
        ctx.attrs.swift_cxx_args,
    )

    # Recursively compile PCMs of transitevely visible exported_deps
    swift_pcm_anon_targets = get_swift_pcm_anon_targets(
        ctx,
        ctx.attrs.dep[SwiftPCMUncompiledInfo].exported_deps,
        ctx.attrs.swift_cxx_args,
    )
    return ctx.actions.anon_targets(sdk_pcm_deps_anon_targets + swift_pcm_anon_targets).promise.map(k)

_swift_pcm_compilation = rule(
    impl = _swift_pcm_compilation_impl,
    attrs = {
        "dep": attrs.dep(),
        "swift_cxx_args": attrs.list(attrs.string(), default = []),
        "_apple_toolchain": attrs.dep(),
    },
)

def compile_underlying_pcm(
        ctx: AnalysisContext,
        uncompiled_pcm_info: SwiftPCMUncompiledInfo,
        compiled_pcm_deps_providers,
        swift_cxx_args: list[str],
        framework_search_path_flags: cmd_args) -> SwiftCompiledModuleInfo:
    module_name = get_module_name(ctx)

    # `compiled_pcm_deps_providers` will contain `WrappedSdkCompiledModuleInfo` providers
    # from direct SDK deps and transitive deps that export sdk deps.
    sdk_deps_tset = get_compiled_sdk_clang_deps_tset(ctx, compiled_pcm_deps_providers)

    # To compile a pcm we only use the exported_deps as those are the only
    # ones that should be transitively exported through public headers
    pcm_deps_tset = get_compiled_pcm_deps_tset(ctx, compiled_pcm_deps_providers)

    cmd, additional_cmd, pcm_output = _get_base_pcm_flags(
        ctx,
        module_name,
        uncompiled_pcm_info,
        sdk_deps_tset,
        pcm_deps_tset,
        swift_cxx_args,
    )
    modulemap_path = uncompiled_pcm_info.exported_preprocessor.modulemap_path
    cmd.add([
        "-Xcc",
        "-I",
        "-Xcc",
        cmd_args([cmd_args(modulemap_path).parent(), "exported_symlink_tree"], delimiter = "/"),
    ])
    cmd.add(framework_search_path_flags)

    _compile_with_argsfile(
        ctx,
        "swift_underlying_pcm_compile",
        module_name,
        cmd,
        additional_cmd,
    )
    return _compiled_module_info(module_name, pcm_output, uncompiled_pcm_info)

def _get_base_pcm_flags(
        ctx: AnalysisContext,
        module_name: str,
        uncompiled_pcm_info: SwiftPCMUncompiledInfo,
        sdk_deps_tset: SwiftCompiledModuleTset,
        pcm_deps_tset: SwiftCompiledModuleTset,
        swift_cxx_args: list[str]) -> (cmd_args, cmd_args, Artifact):
    swift_toolchain = ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info

    cmd = cmd_args()
    cmd.add(get_shared_pcm_compilation_args(module_name))
    cmd.add(["-sdk", swift_toolchain.sdk_path])
    cmd.add(swift_toolchain.compiler_flags)

    if swift_toolchain.resource_dir:
        cmd.add([
            "-resource-dir",
            swift_toolchain.resource_dir,
        ])

    cmd.add(sdk_deps_tset.project_as_args("clang_deps"))
    cmd.add(pcm_deps_tset.project_as_args("clang_deps"))

    modulemap_path = uncompiled_pcm_info.exported_preprocessor.modulemap_path
    pcm_output = ctx.actions.declare_output(module_name + ".pcm")

    additional_cmd = cmd_args(swift_cxx_args)
    additional_cmd.add([
        "-o",
        pcm_output.as_output(),
        modulemap_path,
    ])

    # To correctly resolve modulemap's headers,
    # a search path to the root of modulemap should be passed.
    cmd.add([
        "-Xcc",
        "-I",
        "-Xcc",
        cmd_args(modulemap_path).parent(),
    ])

    # Modular deps like `-Swift.h` have to be materialized.
    cmd.hidden(uncompiled_pcm_info.exported_preprocessor.modular_args)

    return (cmd, additional_cmd, pcm_output)
