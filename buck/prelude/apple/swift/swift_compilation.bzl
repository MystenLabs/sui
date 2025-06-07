# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(
    "@prelude//:artifact_tset.bzl",
    "ArtifactTSet",  # @unused Used as a type
    "make_artifact_tset",
    "project_artifacts",
)
load("@prelude//:paths.bzl", "paths")
load("@prelude//apple:apple_toolchain_types.bzl", "AppleToolchainInfo", "AppleToolsInfo")
load("@prelude//apple:apple_utility.bzl", "get_disable_pch_validation_flags", "get_module_name", "get_versioned_target_triple")
load("@prelude//apple:modulemap.bzl", "preprocessor_info_for_modulemap")
load("@prelude//apple/swift:swift_types.bzl", "SWIFTMODULE_EXTENSION", "SWIFT_EXTENSION")
load("@prelude//cxx:argsfiles.bzl", "CompileArgsfile", "CompileArgsfiles")
load(
    "@prelude//cxx:compile.bzl",
    "CxxSrcWithFlags",  # @unused Used as a type
)
load("@prelude//cxx:headers.bzl", "CHeader")
load(
    "@prelude//cxx:link_groups.bzl",
    "get_link_group",
)
load(
    "@prelude//cxx:preprocessor.bzl",
    "CPreprocessor",
    "CPreprocessorArgs",
    "CPreprocessorInfo",  # @unused Used as a type
    "cxx_inherited_preprocessor_infos",
    "cxx_merge_cpreprocessors",
)
load(
    "@prelude//linking:link_info.bzl",
    "LinkInfo",  # @unused Used as a type
    "SwiftmoduleLinkable",  # @unused Used as a type
)
load("@prelude//utils:arglike.bzl", "ArgLike")
load(":apple_sdk_modules_utility.bzl", "get_compiled_sdk_clang_deps_tset", "get_compiled_sdk_swift_deps_tset", "get_uncompiled_sdk_deps", "is_sdk_modules_provided")
load(
    ":swift_debug_info_utils.bzl",
    "extract_and_merge_clang_debug_infos",
    "extract_and_merge_swift_debug_infos",
)
load(":swift_module_map.bzl", "write_swift_module_map_with_swift_deps")
load(":swift_pcm_compilation.bzl", "compile_underlying_pcm", "get_compiled_pcm_deps_tset", "get_swift_pcm_anon_targets")
load(
    ":swift_pcm_compilation_types.bzl",
    "SwiftPCMUncompiledInfo",
)
load(":swift_sdk_pcm_compilation.bzl", "get_swift_sdk_pcm_anon_targets")
load(":swift_sdk_swiftinterface_compilation.bzl", "get_swift_interface_anon_targets")
load(
    ":swift_toolchain_types.bzl",
    "SwiftCompiledModuleInfo",
    "SwiftCompiledModuleTset",
    "SwiftObjectFormat",
    "SwiftToolchainInfo",
)

# {"module_name": [exported_headers]}, used for Swift header post processing
ExportedHeadersTSet = transitive_set()

SwiftDependencyInfo = provider(fields = {
    "debug_info_tset": provider_field(ArtifactTSet),
    "exported_headers": provider_field(ExportedHeadersTSet),
    # Includes modules through exported_deps, used for compilation
    "exported_swiftmodules": provider_field(SwiftCompiledModuleTset),
})

SwiftCompilationDatabase = record(
    db = field(Artifact),
    other_outputs = field(ArgLike),
)

SwiftCompilationOutput = record(
    # The object file output from compilation.
    object_file = field(Artifact),
    object_format = field(SwiftObjectFormat),
    # The swiftmodule file output from compilation.
    swiftmodule = field(Artifact),
    # The dependency info provider that contains the swiftmodule
    # search paths required for compilation and linking.
    dependency_info = field(SwiftDependencyInfo),
    # Preprocessor info required for ObjC compilation of this library.
    pre = field(CPreprocessor),
    # Exported preprocessor info required for ObjC compilation of rdeps.
    exported_pre = field(CPreprocessor),
    # Argsfiles used to compile object files.
    argsfiles = field(CompileArgsfiles),
    # A tset of (SDK/first-party) swiftmodule artifacts required to be linked into binary.
    # Different outputs of Swift compilation actions might contain duplicates of SDK swiftmodule artifacts.
    swift_debug_info = field(ArtifactTSet),
    # A tset of PCM artifacts used to compile a Swift module.
    clang_debug_info = field(ArtifactTSet),
    # Info required for `[swift-compilation-database]` subtarget.
    compilation_database = field(SwiftCompilationDatabase),
)

SwiftDebugInfo = record(
    static = list[ArtifactTSet],
    shared = list[ArtifactTSet],
)

REQUIRED_SDK_MODULES = ["Swift", "SwiftOnoneSupport", "Darwin", "_Concurrency", "_StringProcessing"]

def get_swift_anonymous_targets(ctx: AnalysisContext, get_apple_library_providers: typing.Callable) -> Promise:
    swift_cxx_flags = get_swift_cxx_flags(ctx)

    # Get SDK deps from direct dependencies,
    # all transitive deps will be compiled recursively.
    direct_uncompiled_sdk_deps = get_uncompiled_sdk_deps(
        ctx.attrs.sdk_modules,
        REQUIRED_SDK_MODULES,
        ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info,
    )

    # Recursively compiling headers of direct and transitive deps as PCM modules,
    # passing apple_library's cxx flags through that must be used for all downward PCM compilations.
    pcm_targets = get_swift_pcm_anon_targets(
        ctx,
        ctx.attrs.deps + ctx.attrs.exported_deps,
        swift_cxx_flags,
    )

    # Recursively compiling SDK's Clang dependencies,
    # passing apple_library's cxx flags through that must be used for all downward PCM compilations.
    sdk_pcm_targets = get_swift_sdk_pcm_anon_targets(
        ctx,
        direct_uncompiled_sdk_deps,
        swift_cxx_flags,
    )

    # Recursively compiling SDK's Swift dependencies,
    # passing apple_library's cxx flags through that must be used for all downward PCM compilations.
    swift_interface_anon_targets = get_swift_interface_anon_targets(
        ctx,
        direct_uncompiled_sdk_deps,
    )
    return ctx.actions.anon_targets(pcm_targets + sdk_pcm_targets + swift_interface_anon_targets).promise.map(get_apple_library_providers)

def _get_explicit_modules_forwards_warnings_as_errors() -> bool:
    return read_root_config("swift", "explicit_modules_forwards_warnings_as_errors", "false").lower() == "true"

def get_swift_cxx_flags(ctx: AnalysisContext) -> list[str]:
    """Iterates through `swift_compiler_flags` and returns a list of flags that might affect Clang compilation"""
    gather, next = ([], False)

    # Each target needs to propagate the compilers target triple.
    # This can vary depending on the deployment target of each library.
    gather += ["-target", get_versioned_target_triple(ctx)]

    for f in ctx.attrs.swift_compiler_flags:
        if next:
            gather.append("-Xcc")
            gather.append(str(f).replace('\"', ""))
            next = False
        elif str(f) == "\"-Xcc\"":
            next = True
        elif _get_explicit_modules_forwards_warnings_as_errors() and str(f) == "\"-warnings-as-errors\"":
            gather.append("-warnings-as-errors")

    if ctx.attrs.enable_cxx_interop:
        gather += ["-Xfrontend", "-enable-cxx-interop"]

    if ctx.attrs.swift_version != None:
        gather += ["-swift-version", ctx.attrs.swift_version]

    return gather

def compile_swift(
        ctx: AnalysisContext,
        srcs: list[CxxSrcWithFlags],
        parse_as_library: bool,
        deps_providers: list,
        exported_headers: list[CHeader],
        objc_modulemap_pp_info: [CPreprocessor, None],
        framework_search_paths_flags: cmd_args,
        extra_search_paths_flags: list[ArgLike] = []) -> [SwiftCompilationOutput, None]:
    if not srcs:
        return None

    # If this target imports XCTest we need to pass the search path to its swiftmodule.
    framework_search_paths = cmd_args()
    framework_search_paths.add(_get_xctest_swiftmodule_search_path(ctx))

    # Pass the framework search paths to the driver and clang importer. This is required
    # for pcm compilation, which does not pass through driver search paths.
    framework_search_paths.add(framework_search_paths_flags)
    framework_search_paths.add(cmd_args(framework_search_paths_flags, prepend = "-Xcc"))

    # If a target exports ObjC headers and Swift explicit modules are enabled,
    # we need to precompile a PCM of the underlying module and supply it to the Swift compilation.
    if objc_modulemap_pp_info and ctx.attrs.uses_explicit_modules:
        underlying_swift_pcm_uncompiled_info = get_swift_pcm_uncompile_info(
            ctx,
            None,
            objc_modulemap_pp_info,
        )
        if underlying_swift_pcm_uncompiled_info:
            compiled_underlying_pcm = compile_underlying_pcm(
                ctx,
                underlying_swift_pcm_uncompiled_info,
                deps_providers,
                get_swift_cxx_flags(ctx),
                framework_search_paths,
            )
        else:
            compiled_underlying_pcm = None
    else:
        compiled_underlying_pcm = None

    toolchain = ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info

    module_name = get_module_name(ctx)
    output_header = ctx.actions.declare_output(module_name + "-Swift.h")
    output_object = ctx.actions.declare_output(module_name + ".o")
    output_swiftmodule = ctx.actions.declare_output(module_name + SWIFTMODULE_EXTENSION)

    shared_flags = _get_shared_flags(
        ctx,
        deps_providers,
        parse_as_library,
        compiled_underlying_pcm,
        module_name,
        exported_headers,
        objc_modulemap_pp_info,
        extra_search_paths_flags,
    )
    shared_flags.add(framework_search_paths)

    if toolchain.can_toolchain_emit_obj_c_header_textually:
        _compile_swiftmodule(ctx, toolchain, shared_flags, srcs, output_swiftmodule, output_header)
    else:
        unprocessed_header = ctx.actions.declare_output(module_name + "-SwiftUnprocessed.h")
        _compile_swiftmodule(ctx, toolchain, shared_flags, srcs, output_swiftmodule, unprocessed_header)
        _perform_swift_postprocessing(ctx, module_name, unprocessed_header, output_header)

    argsfiles = _compile_object(ctx, toolchain, shared_flags, srcs, output_object)

    # Swift libraries extend the ObjC modulemaps to include the -Swift.h header
    modulemap_pp_info = preprocessor_info_for_modulemap(ctx, "swift-extended", exported_headers, output_header)
    exported_swift_header = CHeader(
        artifact = output_header,
        name = output_header.basename,
        namespace = module_name,
        named = False,
    )
    exported_pp_info = CPreprocessor(
        headers = [exported_swift_header],
        modular_args = modulemap_pp_info.modular_args,
        relative_args = CPreprocessorArgs(args = modulemap_pp_info.relative_args.args),
        modulemap_path = modulemap_pp_info.modulemap_path,
    )

    # We also need to include the unprefixed -Swift.h header in this libraries preprocessor info
    swift_header = CHeader(
        artifact = output_header,
        name = output_header.basename,
        namespace = "",
        named = False,
    )
    pre = CPreprocessor(headers = [swift_header])

    # Pass up the swiftmodule paths for this module and its exported_deps
    return SwiftCompilationOutput(
        object_file = output_object,
        object_format = toolchain.object_format,
        swiftmodule = output_swiftmodule,
        dependency_info = get_swift_dependency_info(ctx, exported_pp_info, output_swiftmodule, deps_providers),
        pre = pre,
        exported_pre = exported_pp_info,
        argsfiles = argsfiles,
        swift_debug_info = extract_and_merge_swift_debug_infos(ctx, deps_providers, [output_swiftmodule]),
        clang_debug_info = extract_and_merge_clang_debug_infos(ctx, deps_providers),
        compilation_database = _create_compilation_database(ctx, srcs, argsfiles.absolute[SWIFT_EXTENSION]),
    )

# Swift headers are postprocessed to make them compatible with Objective-C
# compilation that does not use -fmodules. This is a workaround for the bad
# performance of -fmodules without Explicit Modules, once Explicit Modules is
# supported, this postprocessing should be removed.
def _perform_swift_postprocessing(
        ctx: AnalysisContext,
        module_name: str,
        unprocessed_header: Artifact,
        output_header: Artifact):
    transitive_exported_headers = {
        module: module_exported_headers
        for exported_headers_map in _get_exported_headers_tset(ctx).traverse()
        if exported_headers_map
        for module, module_exported_headers in exported_headers_map.items()
    }
    deps_json = ctx.actions.write_json(module_name + "-Deps.json", transitive_exported_headers)
    postprocess_cmd = cmd_args(ctx.attrs._apple_tools[AppleToolsInfo].swift_objc_header_postprocess)
    postprocess_cmd.add([
        unprocessed_header,
        deps_json,
        output_header.as_output(),
    ])
    ctx.actions.run(postprocess_cmd, category = "swift_objc_header_postprocess")

# We use separate actions for swiftmodule and object file output. This
# improves build parallelism at the cost of duplicated work, but by disabling
# type checking in function bodies the swiftmodule compilation can be done much
# faster than object file output.
def _compile_swiftmodule(
        ctx: AnalysisContext,
        toolchain: SwiftToolchainInfo,
        shared_flags: cmd_args,
        srcs: list[CxxSrcWithFlags],
        output_swiftmodule: Artifact,
        output_header: Artifact) -> CompileArgsfiles:
    argfile_cmd = cmd_args(shared_flags)
    argfile_cmd.add([
        "-Xfrontend",
        "-experimental-skip-non-inlinable-function-bodies-without-types",
        "-emit-module",
        "-emit-objc-header",
    ])
    cmd = cmd_args([
        "-emit-module-path",
        output_swiftmodule.as_output(),
        "-emit-objc-header-path",
        output_header.as_output(),
    ])
    return _compile_with_argsfile(ctx, "swiftmodule_compile", SWIFTMODULE_EXTENSION, argfile_cmd, srcs, cmd, toolchain)

def _compile_object(
        ctx: AnalysisContext,
        toolchain: SwiftToolchainInfo,
        shared_flags: cmd_args,
        srcs: list[CxxSrcWithFlags],
        output_object: Artifact) -> CompileArgsfiles:
    object_format = toolchain.object_format.value
    embed_bitcode = False
    if toolchain.object_format == SwiftObjectFormat("object-embed-bitcode"):
        object_format = "object"
        embed_bitcode = True

    cmd = cmd_args([
        "-emit-{}".format(object_format),
        "-o",
        output_object.as_output(),
    ])

    if embed_bitcode:
        cmd.add("--embed-bitcode")

    return _compile_with_argsfile(ctx, "swift_compile", SWIFT_EXTENSION, shared_flags, srcs, cmd, toolchain)

def _compile_with_argsfile(
        ctx: AnalysisContext,
        category_prefix: str,
        extension: str,
        shared_flags: cmd_args,
        srcs: list[CxxSrcWithFlags],
        additional_flags: cmd_args,
        toolchain: SwiftToolchainInfo) -> CompileArgsfiles:
    shell_quoted_args = cmd_args(shared_flags, quote = "shell")
    argsfile, _ = ctx.actions.write(extension + ".argsfile", shell_quoted_args, allow_args = True)
    input_args = [shared_flags]
    cmd_form = cmd_args(cmd_args(argsfile, format = "@{}", delimiter = "")).hidden(input_args)
    cmd_form.add([s.file for s in srcs])

    cmd = cmd_args(toolchain.compiler)
    cmd.add(additional_flags)
    cmd.add(cmd_form)

    # If we prefer to execute locally (e.g., for perf reasons), ensure we upload to the cache,
    # so that CI builds populate caches used by developer machines.
    explicit_modules_enabled = uses_explicit_modules(ctx)

    # Make it easier to debug whether Swift actions get compiled with explicit modules or not
    category = category_prefix + ("_with_explicit_mods" if explicit_modules_enabled else "")
    ctx.actions.run(
        cmd,
        category = category,
        # Swift compilation on RE without explicit modules is impractically expensive
        # because there's no shared module cache across different libraries.
        prefer_local = not explicit_modules_enabled,
        allow_cache_upload = True,
    )

    relative_argsfile = CompileArgsfile(
        file = argsfile,
        cmd_form = cmd_form,
        input_args = input_args,
        args = shell_quoted_args,
        args_without_file_prefix_args = shared_flags,
    )

    # Swift correctly handles relative paths and we can utilize the relative argsfile for absolute paths.
    return CompileArgsfiles(relative = {extension: relative_argsfile}, absolute = {extension: relative_argsfile})

def _get_shared_flags(
        ctx: AnalysisContext,
        deps_providers: list,
        parse_as_library: bool,
        underlying_module: [SwiftCompiledModuleInfo, None],
        module_name: str,
        objc_headers: list[CHeader],
        objc_modulemap_pp_info: [CPreprocessor, None],
        extra_search_paths_flags: list[ArgLike] = []) -> cmd_args:
    toolchain = ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info
    cmd = cmd_args()
    cmd.add([
        # Setting this to empty will get the driver to make all paths absolute when
        # passed to the frontend. We later debug prefix these to ensure relative paths
        # in the debug info.
        "-working-directory=",
        # Unset the working directory in the debug information.
        "-file-compilation-dir",
        ".",
        "-sdk",
        toolchain.sdk_path,
        "-target",
        get_versioned_target_triple(ctx),
        "-wmo",
        "-module-name",
        module_name,
        "-Xfrontend",
        "-enable-cross-import-overlays",
    ])

    if parse_as_library:
        cmd.add([
            "-parse-as-library",
        ])

    if uses_explicit_modules(ctx):
        # We set -fmodule-file-home-is-cwd as this is used to correctly
        # set the working directory of modules when generating debug info.
        cmd.add([
            "-Xcc",
            "-Xclang",
            "-Xcc",
            "-fmodule-file-home-is-cwd",
        ])
        cmd.add(get_disable_pch_validation_flags())

    if toolchain.resource_dir:
        cmd.add([
            "-resource-dir",
            toolchain.resource_dir,
        ])

    if ctx.attrs.swift_version:
        cmd.add(["-swift-version", ctx.attrs.swift_version])

    if ctx.attrs.enable_cxx_interop:
        if toolchain.supports_swift_cxx_interoperability_mode:
            cmd.add(["-cxx-interoperability-mode=default"])
        else:
            cmd.add(["-enable-experimental-cxx-interop"])

    serialize_debugging_options = False
    if ctx.attrs.serialize_debugging_options:
        if objc_headers:
            # TODO(T99100029): We cannot use VFS overlays with Buck2, so we have to disable
            # serializing debugging options for mixed libraries to debug successfully
            warning("Mixed libraries cannot serialize debugging options, disabling for module `{}` in rule `{}`".format(module_name, ctx.label))
        elif not toolchain.prefix_serialized_debugging_options:
            warning("The current toolchain does not support prefixing serialized debugging options, disabling for module `{}` in rule `{}`".format(module_name, ctx.label))
        else:
            # Apply the debug prefix map to Swift serialized debugging info.
            # This will allow for debugging remotely built swiftmodule files.
            serialize_debugging_options = True

    if serialize_debugging_options:
        cmd.add([
            "-Xfrontend",
            "-serialize-debugging-options",
            "-Xfrontend",
            "-prefix-serialized-debugging-options",
        ])
    else:
        cmd.add([
            "-Xfrontend",
            "-no-serialize-debugging-options",
        ])

    if toolchain.can_toolchain_emit_obj_c_header_textually:
        cmd.add([
            "-Xfrontend",
            "-emit-clang-header-nonmodular-includes",
        ])

    if toolchain.supports_cxx_interop_requirement_at_import:
        cmd.add([
            "-Xfrontend",
            "-disable-cxx-interop-requirement-at-import",
        ])

    if toolchain.supports_swift_importing_objc_forward_declarations:
        cmd.add([
            "-Xfrontend",
            "-enable-upcoming-feature",
            "-Xfrontend",
            "ImportObjcForwardDeclarations",
        ])

    pcm_deps_tset = get_compiled_pcm_deps_tset(ctx, deps_providers)
    sdk_clang_deps_tset = get_compiled_sdk_clang_deps_tset(ctx, deps_providers)
    sdk_swift_deps_tset = get_compiled_sdk_swift_deps_tset(ctx, deps_providers)

    # Add flags required to import ObjC module dependencies
    _add_clang_deps_flags(ctx, pcm_deps_tset, sdk_clang_deps_tset, cmd)
    _add_swift_deps_flags(ctx, sdk_swift_deps_tset, cmd)

    # Add flags for importing the ObjC part of this library
    _add_mixed_library_flags_to_cmd(ctx, cmd, underlying_module, objc_headers, objc_modulemap_pp_info)

    # Add toolchain and target flags last to allow for overriding defaults
    cmd.add(toolchain.compiler_flags)
    cmd.add(ctx.attrs.swift_compiler_flags)
    cmd.add(extra_search_paths_flags)

    return cmd

def _add_swift_deps_flags(
        ctx: AnalysisContext,
        sdk_deps_tset: SwiftCompiledModuleTset,
        cmd: cmd_args):
    # If Explicit Modules are enabled, a few things must be provided to a compilation job:
    # 1. Direct and transitive SDK deps from `sdk_modules` attribute.
    # 2. Direct and transitive user-defined deps.
    # 3. Transitive SDK deps of user-defined deps.
    # (This is the case, when a user-defined dep exports a type from SDK module,
    # thus such SDK module should be implicitly visible to consumers of that custom dep)
    if uses_explicit_modules(ctx):
        module_name = get_module_name(ctx)
        swift_deps_tset = ctx.actions.tset(
            SwiftCompiledModuleTset,
            children = _get_swift_paths_tsets(ctx.attrs.deps + ctx.attrs.exported_deps),
        )
        swift_module_map_artifact = write_swift_module_map_with_swift_deps(
            ctx,
            module_name,
            sdk_deps_tset,
            swift_deps_tset,
        )
        cmd.add([
            "-Xcc",
            "-fno-implicit-modules",
            "-Xcc",
            "-fno-implicit-module-maps",
            "-Xfrontend",
            "-disable-implicit-swift-modules",
            "-Xfrontend",
            "-explicit-swift-module-map-file",
            "-Xfrontend",
            swift_module_map_artifact,
        ])
    else:
        depset = ctx.actions.tset(SwiftCompiledModuleTset, children = _get_swift_paths_tsets(ctx.attrs.deps + ctx.attrs.exported_deps))
        cmd.add(depset.project_as_args("module_search_path"))

def _add_clang_deps_flags(
        ctx: AnalysisContext,
        pcm_deps_tset: SwiftCompiledModuleTset,
        sdk_deps_tset: SwiftCompiledModuleTset,
        cmd: cmd_args) -> None:
    # If a module uses Explicit Modules, all direct and
    # transitive Clang deps have to be explicitly added.
    if uses_explicit_modules(ctx):
        cmd.add(pcm_deps_tset.project_as_args("clang_deps"))

        # Add Clang sdk modules which do not go to swift modulemap
        cmd.add(sdk_deps_tset.project_as_args("clang_deps"))
    else:
        inherited_preprocessor_infos = cxx_inherited_preprocessor_infos(ctx.attrs.deps + ctx.attrs.exported_deps)
        preprocessors = cxx_merge_cpreprocessors(ctx, [], inherited_preprocessor_infos)
        cmd.add(cmd_args(preprocessors.set.project_as_args("args"), prepend = "-Xcc"))
        cmd.add(cmd_args(preprocessors.set.project_as_args("modular_args"), prepend = "-Xcc"))
        cmd.add(cmd_args(preprocessors.set.project_as_args("include_dirs"), prepend = "-Xcc"))

def _add_mixed_library_flags_to_cmd(
        ctx: AnalysisContext,
        cmd: cmd_args,
        underlying_module: [SwiftCompiledModuleInfo, None],
        objc_headers: list[CHeader],
        objc_modulemap_pp_info: [CPreprocessor, None]) -> None:
    if uses_explicit_modules(ctx):
        if underlying_module:
            cmd.add(underlying_module.clang_importer_args)
            cmd.add("-import-underlying-module")
        return

    if not objc_headers:
        return

    # TODO(T99100029): We cannot use VFS overlays to mask this import from
    # the debugger as they require absolute paths. Instead we will enforce
    # that mixed libraries do not have serialized debugging info and rely on
    # rdeps to serialize the correct paths.
    for arg in objc_modulemap_pp_info.relative_args.args:
        cmd.add("-Xcc")
        cmd.add(arg)

    for arg in objc_modulemap_pp_info.modular_args:
        cmd.add("-Xcc")
        cmd.add(arg)

    cmd.add("-import-underlying-module")

def _get_swift_paths_tsets(deps: list[Dependency]) -> list[SwiftCompiledModuleTset]:
    return [
        d[SwiftDependencyInfo].exported_swiftmodules
        for d in deps
        if SwiftDependencyInfo in d
    ]

def _get_external_debug_info_tsets(deps: list[Dependency]) -> list[ArtifactTSet]:
    return [
        d[SwiftDependencyInfo].debug_info_tset
        for d in deps
        if SwiftDependencyInfo in d
    ]

def _get_exported_headers_tset(ctx: AnalysisContext, exported_headers: [list[str], None] = None) -> ExportedHeadersTSet:
    return ctx.actions.tset(
        ExportedHeadersTSet,
        value = {get_module_name(ctx): exported_headers} if exported_headers else None,
        children = [
            dep.exported_headers
            for dep in [x.get(SwiftDependencyInfo) for x in ctx.attrs.exported_deps]
            if dep and dep.exported_headers
        ],
    )

def get_swift_pcm_uncompile_info(
        ctx: AnalysisContext,
        propagated_exported_preprocessor_info: [CPreprocessorInfo, None],
        exported_pre: [CPreprocessor, None]) -> [SwiftPCMUncompiledInfo, None]:
    swift_toolchain = ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info

    if is_sdk_modules_provided(swift_toolchain):
        propagated_pp_args_cmd = cmd_args(propagated_exported_preprocessor_info.set.project_as_args("args"), prepend = "-Xcc") if propagated_exported_preprocessor_info else None
        return SwiftPCMUncompiledInfo(
            name = get_module_name(ctx),
            is_transient = not ctx.attrs.modular or not exported_pre,
            exported_preprocessor = exported_pre,
            exported_deps = ctx.attrs.exported_deps,
            propagated_preprocessor_args_cmd = propagated_pp_args_cmd,
            uncompiled_sdk_modules = ctx.attrs.sdk_modules,
        )
    return None

def get_swift_dependency_info(
        ctx: AnalysisContext,
        exported_pre: [CPreprocessor, None],
        output_module: [Artifact, None],
        deps_providers: list) -> SwiftDependencyInfo:
    all_deps = ctx.attrs.exported_deps + ctx.attrs.deps
    if ctx.attrs.reexport_all_header_dependencies:
        exported_deps = all_deps
    else:
        exported_deps = ctx.attrs.exported_deps

    # We only need to pass up the exported_headers for Swift header post-processing.
    # If the toolchain can emit textual imports already then we skip the extra work.
    exported_headers = []
    if not ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info.can_toolchain_emit_obj_c_header_textually:
        exported_headers = [_header_basename(header) for header in ctx.attrs.exported_headers]
        exported_headers += [header.name for header in exported_pre.headers] if exported_pre else []

    # We pass through the SDK swiftmodules here to match Buck 1 behaviour. This is
    # pretty loose, but it matches Buck 1 behavior so cannot be improved until
    # migration is complete.
    transitive_swiftmodule_deps = _get_swift_paths_tsets(exported_deps) + [get_compiled_sdk_swift_deps_tset(ctx, deps_providers)]
    if output_module:
        compiled_info = SwiftCompiledModuleInfo(
            is_framework = False,
            is_swiftmodule = True,
            module_name = get_module_name(ctx),
            output_artifact = output_module,
        )
        exported_swiftmodules = ctx.actions.tset(SwiftCompiledModuleTset, value = compiled_info, children = transitive_swiftmodule_deps)
    else:
        exported_swiftmodules = ctx.actions.tset(SwiftCompiledModuleTset, children = transitive_swiftmodule_deps)

    debug_info_tset = make_artifact_tset(
        actions = ctx.actions,
        artifacts = [output_module] if output_module != None else [],
        children = _get_external_debug_info_tsets(all_deps),
        label = ctx.label,
    )

    return SwiftDependencyInfo(
        debug_info_tset = debug_info_tset,
        exported_headers = _get_exported_headers_tset(ctx, exported_headers),
        exported_swiftmodules = exported_swiftmodules,
    )

def _header_basename(header: [Artifact, str]) -> str:
    if type(header) == type(""):
        return paths.basename(header)
    else:
        return header.basename

def uses_explicit_modules(ctx: AnalysisContext) -> bool:
    swift_toolchain = ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info
    return ctx.attrs.uses_explicit_modules and is_sdk_modules_provided(swift_toolchain)

def get_swiftmodule_linkable(swift_compile_output: [SwiftCompilationOutput, None]) -> [SwiftmoduleLinkable, None]:
    return SwiftmoduleLinkable(swiftmodules = swift_compile_output.swift_debug_info) if swift_compile_output else None

def extract_swiftmodule_linkables(link_infos: [list[LinkInfo], None]) -> list[SwiftmoduleLinkable]:
    linkables = []
    for info in link_infos:
        for linkable in info.linkables:
            if isinstance(linkable, SwiftmoduleLinkable):
                linkables.append(linkable)

    return linkables

def get_swiftmodule_linker_flags(ctx: AnalysisContext, swiftmodule_linkable: [SwiftmoduleLinkable, None]) -> cmd_args:
    if swiftmodule_linkable:
        tset = swiftmodule_linkable.swiftmodules
        artifacts = project_artifacts(
            actions = ctx.actions,
            tsets = [tset],
        )
        return cmd_args([cmd_args(swiftmodule, format = "-Wl,-add_ast_path,{}") for swiftmodule in artifacts])
    return cmd_args()

def _get_xctest_swiftmodule_search_path(ctx: AnalysisContext) -> cmd_args:
    # With explicit modules we don't need to search at all.
    if uses_explicit_modules(ctx):
        return cmd_args()

    for fw in ctx.attrs.frameworks:
        if fw.endswith("XCTest.framework"):
            toolchain = ctx.attrs._apple_toolchain[AppleToolchainInfo]
            return cmd_args(toolchain.platform_path, format = "-I{}/Developer/usr/lib")

    return cmd_args()

def get_swift_debug_infos(
        ctx: AnalysisContext,
        swift_dependency_info: [SwiftDependencyInfo, None],
        swift_output: [SwiftCompilationOutput, None]) -> SwiftDebugInfo:
    # When determing the debug info for shared libraries, if the shared library is a link group, we rely on the link group links to
    # obtain the debug info for linked libraries and only need to provide any swift debug info for this library itself. Otherwise
    # if linking standard shared, we need to obtain the transitive debug info.
    if get_link_group(ctx):
        swift_shared_debug_info = []
    else:
        swift_shared_debug_info = _get_swift_shared_debug_info(swift_dependency_info) if swift_dependency_info else []

    clang_debug_info = [swift_output.clang_debug_info] if swift_output else []
    swift_debug_info = [swift_output.swift_debug_info] if swift_output else []

    return SwiftDebugInfo(
        static = clang_debug_info + swift_debug_info,
        shared = swift_shared_debug_info + clang_debug_info + swift_debug_info,
    )

def _get_swift_shared_debug_info(swift_dependency_info: SwiftDependencyInfo) -> list[ArtifactTSet]:
    return [swift_dependency_info.debug_info_tset] if swift_dependency_info.debug_info_tset else []

def _get_project_root_file(ctx) -> Artifact:
    content = cmd_args(ctx.label.project_root)
    return ctx.actions.write("project_root_file", content, absolute = True)

def _create_compilation_database(
        ctx: AnalysisContext,
        srcs: list[CxxSrcWithFlags],
        argfile: CompileArgsfile) -> SwiftCompilationDatabase:
    module_name = get_module_name(ctx)

    swift_toolchain = ctx.attrs._apple_toolchain[AppleToolchainInfo].swift_toolchain_info
    mk_comp_db = swift_toolchain.mk_swift_comp_db[RunInfo]

    identifier = module_name + ".swift_comp_db.json"
    cdb_artifact = ctx.actions.declare_output(identifier)
    cmd = cmd_args(mk_comp_db)
    cmd.add(cmd_args(cdb_artifact.as_output(), format = "--output={}"))
    cmd.add(cmd_args(_get_project_root_file(ctx), format = "--project-root-file={}"))
    cmd.add(["--files"] + [s.file for s in srcs])

    cmd.add("--")
    cmd.add(argfile.cmd_form)
    ctx.actions.run(cmd, category = "swift_compilation_database", identifier = identifier)

    return SwiftCompilationDatabase(db = cdb_artifact, other_outputs = argfile.cmd_form)
