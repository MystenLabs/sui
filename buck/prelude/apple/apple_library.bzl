# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(
    "@prelude//:artifact_tset.bzl",
    "project_artifacts",
)
load("@prelude//apple:apple_dsym.bzl", "DSYM_SUBTARGET", "get_apple_dsym")
load("@prelude//apple:apple_stripping.bzl", "apple_strip_args")
# @oss-disable: load("@prelude//apple/meta_only:linker_outputs.bzl", "add_extra_linker_outputs") 
load(
    "@prelude//apple/swift:swift_compilation.bzl",
    "compile_swift",
    "get_swift_anonymous_targets",
    "get_swift_debug_infos",
    "get_swift_dependency_info",
    "get_swift_pcm_uncompile_info",
    "get_swiftmodule_linkable",
    "uses_explicit_modules",
)
load("@prelude//apple/swift:swift_types.bzl", "SWIFT_EXTENSION")
load(
    "@prelude//cxx:argsfiles.bzl",
    "CompileArgsfile",  # @unused Used as a type
    "CompileArgsfiles",
)
load(
    "@prelude//cxx:compile.bzl",
    "CxxSrcWithFlags",  # @unused Used as a type
)
load(
    "@prelude//cxx:cxx_library.bzl",
    "CxxLibraryOutput",  # @unused Used as a type
    "cxx_library_parameterized",
)
load(
    "@prelude//cxx:cxx_library_utility.bzl",
    "cxx_attr_deps",
    "cxx_attr_exported_deps",
)
load("@prelude//cxx:cxx_sources.bzl", "get_srcs_with_flags")
load(
    "@prelude//cxx:cxx_types.bzl",
    "CxxRuleAdditionalParams",
    "CxxRuleConstructorParams",
    "CxxRuleProviderParams",
    "CxxRuleSubTargetParams",
)
load("@prelude//cxx:headers.bzl", "cxx_attr_exported_headers")
load(
    "@prelude//cxx:linker.bzl",
    "SharedLibraryFlagOverrides",
)
load(
    "@prelude//cxx:preprocessor.bzl",
    "CPreprocessor",
    "CPreprocessorArgs",
    "CPreprocessorInfo",  # @unused Used as a type
)
load(
    "@prelude//linking:link_info.bzl",
    "LibOutputStyle",
)
load("@prelude//utils:arglike.bzl", "ArgLike")
load("@prelude//utils:utils.bzl", "expect")
load(":apple_bundle_types.bzl", "AppleBundleLinkerMapInfo", "AppleMinDeploymentVersionInfo")
load(":apple_frameworks.bzl", "get_framework_search_path_flags")
load(":apple_genrule_deps.bzl", "get_apple_build_genrule_deps_attr_value", "get_apple_genrule_deps_outputs")
load(":apple_modular_utility.bzl", "MODULE_CACHE_PATH")
load(":apple_target_sdk_version.bzl", "get_min_deployment_version_for_node", "get_min_deployment_version_target_linker_flags", "get_min_deployment_version_target_preprocessor_flags")
load(":apple_utility.bzl", "get_apple_cxx_headers_layout", "get_apple_stripped_attr_value_with_default_fallback", "get_module_name")
load(
    ":debug.bzl",
    "AppleDebuggableInfo",
    "DEBUGINFO_SUBTARGET",
)
load(":modulemap.bzl", "preprocessor_info_for_modulemap")
load(":resource_groups.bzl", "create_resource_graph")
load(":xcode.bzl", "apple_populate_xcode_attributes")
load(":xctest_swift_support.bzl", "xctest_swift_support_info")

AppleLibraryAdditionalParams = record(
    # Name of the top level rule utilizing the apple_library rule.
    rule_type = str,
    # Extra flags to be passed to the linker.
    extra_exported_link_flags = field(list[ArgLike], []),
    # Extra flags to be passed to the Swift compiler.
    extra_swift_compiler_flags = field(list[ArgLike], []),
    # Linker flags that tell the linker to create shared libraries, overriding the default shared library flags.
    # e.g. when building Apple tests, we want to link with `-bundle` instead of `-shared` to allow
    # linking against the bundle loader.
    shared_library_flags = field([SharedLibraryFlagOverrides, None], None),
    # Function to use for setting Xcode attributes for the Xcode data sub target.
    populate_xcode_attributes_func = field(typing.Callable, apple_populate_xcode_attributes),
    # Define which sub targets to generate.
    generate_sub_targets = field(CxxRuleSubTargetParams, CxxRuleSubTargetParams()),
    # Define which providers to generate.
    generate_providers = field(CxxRuleProviderParams, CxxRuleProviderParams()),
    # Forces link group linking logic, even when there's no mapping. Link group linking
    # without a mapping is equivalent to statically linking the whole transitive dep graph.
    force_link_group_linking = field(bool, False),
)

def apple_library_impl(ctx: AnalysisContext) -> [Promise, list[Provider]]:
    def get_apple_library_providers(deps_providers) -> list[Provider]:
        constructor_params = apple_library_rule_constructor_params_and_swift_providers(
            ctx,
            AppleLibraryAdditionalParams(
                rule_type = "apple_library",
                generate_providers = CxxRuleProviderParams(
                    java_packaging_info = False,
                    android_packageable_info = False,
                    omnibus_root = False,
                    # We generate a provider on our own, disable to avoid several providers of same type.
                    cxx_resources_as_apple_resources = False,
                ),
            ),
            deps_providers,
        )
        output = cxx_library_parameterized(ctx, constructor_params)
        return output.providers

    if uses_explicit_modules(ctx):
        return get_swift_anonymous_targets(ctx, get_apple_library_providers)
    else:
        return get_apple_library_providers([])

def apple_library_rule_constructor_params_and_swift_providers(ctx: AnalysisContext, params: AppleLibraryAdditionalParams, deps_providers: list = [], is_test_target: bool = False) -> CxxRuleConstructorParams:
    cxx_srcs, swift_srcs = _filter_swift_srcs(ctx)

    # First create a modulemap if necessary. This is required for importing
    # ObjC code in Swift so must be done before Swift compilation.
    exported_hdrs = cxx_attr_exported_headers(ctx, get_apple_cxx_headers_layout(ctx))
    if (ctx.attrs.modular or swift_srcs) and exported_hdrs:
        modulemap_pre = preprocessor_info_for_modulemap(ctx, "exported", exported_hdrs, None)
    else:
        modulemap_pre = None

    framework_search_paths_flags = get_framework_search_path_flags(ctx)
    swift_compile = compile_swift(
        ctx,
        swift_srcs,
        True,  # parse_as_library
        deps_providers,
        exported_hdrs,
        modulemap_pre,
        framework_search_paths_flags,
        params.extra_swift_compiler_flags,
    )
    swift_object_files = [swift_compile.object_file] if swift_compile else []

    swift_pre = CPreprocessor()
    if swift_compile:
        # If we have Swift we export the extended modulemap that includes
        # the ObjC exported headers and the -Swift.h header.
        exported_pre = swift_compile.exported_pre

        # We also include the -Swift.h header to this libraries preprocessor
        # info, so that we can import it unprefixed in this module.
        swift_pre = swift_compile.pre
    elif modulemap_pre:
        # Otherwise if this library is modular we export a modulemap of
        # the ObjC exported headers.
        exported_pre = modulemap_pre
    else:
        exported_pre = None

    swift_dependency_info = swift_compile.dependency_info if swift_compile else get_swift_dependency_info(ctx, None, None, deps_providers)
    swift_debug_info = get_swift_debug_infos(
        ctx,
        swift_dependency_info,
        swift_compile,
    )

    modular_pre = CPreprocessor(
        uses_modules = ctx.attrs.uses_modules,
        modular_args = [
            "-fcxx-modules",
            "-fmodules",
            "-fmodule-name=" + get_module_name(ctx),
            "-fmodules-cache-path=" + MODULE_CACHE_PATH,
            # TODO(T123756899): We have to use this hack to make compilation work
            # when Clang modules are enabled and using toolchains. That's because
            # resource-dir is passed as a relative path (so that no abs paths appear
            # in any .pcm). The compiler will then expand and generate #include paths
            # that won't work unless we have the directive below.
            "-I.",
        ],
    )

    contains_swift_sources = bool(swift_srcs)
    xctest_swift_support_provider = xctest_swift_support_info(ctx, contains_swift_sources, is_test_target)

    def additional_providers_factory(propagated_exported_preprocessor_info: [CPreprocessorInfo, None]) -> list[Provider]:
        # Expose `SwiftPCMUncompiledInfo` which represents the ObjC part of a target,
        # if a target also has a Swift part, the provider will expose the generated `-Swift.h` header.
        # This is used for Swift Explicit Modules, and allows compiling a PCM file out of the exported headers.
        swift_pcm_uncompile_info = get_swift_pcm_uncompile_info(
            ctx,
            propagated_exported_preprocessor_info,
            exported_pre,
        )
        providers = [swift_pcm_uncompile_info] if swift_pcm_uncompile_info else []
        providers.append(swift_dependency_info)
        providers.append(xctest_swift_support_provider)
        return providers

    framework_search_path_pre = CPreprocessor(
        relative_args = CPreprocessorArgs(args = [framework_search_paths_flags]),
    )

    genrule_deps_outputs = []
    if get_apple_build_genrule_deps_attr_value(ctx):
        genrule_deps_outputs = get_apple_genrule_deps_outputs(cxx_attr_deps(ctx) + cxx_attr_exported_deps(ctx))

    return CxxRuleConstructorParams(
        rule_type = params.rule_type,
        is_test = (params.rule_type == "apple_test"),
        headers_layout = get_apple_cxx_headers_layout(ctx),
        extra_exported_link_flags = params.extra_exported_link_flags,
        extra_hidden = genrule_deps_outputs,
        extra_link_flags = [_get_linker_flags(ctx)],
        extra_link_input = swift_object_files,
        extra_link_input_has_external_debug_info = True,
        extra_preprocessors = get_min_deployment_version_target_preprocessor_flags(ctx) + [swift_pre, modular_pre],
        extra_exported_preprocessors = filter(None, [framework_search_path_pre, exported_pre]),
        srcs = cxx_srcs,
        additional = CxxRuleAdditionalParams(
            srcs = swift_srcs,
            argsfiles = swift_compile.argsfiles if swift_compile else CompileArgsfiles(),
            # We need to add any swift modules that we include in the link, as
            # these will end up as `N_AST` entries that `dsymutil` will need to
            # follow.
            static_external_debug_info = swift_debug_info.static,
            shared_external_debug_info = swift_debug_info.shared,
            subtargets = {
                "swift-compilation-database": [
                    DefaultInfo(
                        default_output = swift_compile.compilation_database.db if swift_compile else None,
                        other_outputs = [swift_compile.compilation_database.other_outputs] if swift_compile else [],
                    ),
                ],
                "swift-compile": [DefaultInfo(default_output = swift_compile.object_file if swift_compile else None)],
            },
            additional_providers_factory = additional_providers_factory,
        ),
        output_style_sub_targets_and_providers_factory = _get_link_style_sub_targets_and_providers,
        shared_library_flags = params.shared_library_flags,
        # apple_library's 'stripped' arg only applies to shared subtargets, or,
        # targets with 'preferred_linkage = "shared"'
        strip_executable = get_apple_stripped_attr_value_with_default_fallback(ctx),
        strip_args_factory = apple_strip_args,
        force_link_group_linking = params.force_link_group_linking,
        cxx_populate_xcode_attributes_func = lambda local_ctx, **kwargs: _xcode_populate_attributes(ctx = local_ctx, populate_xcode_attributes_func = params.populate_xcode_attributes_func, **kwargs),
        generate_sub_targets = params.generate_sub_targets,
        generate_providers = params.generate_providers,
        # Some apple rules rely on `static` libs *not* following dependents.
        link_groups_force_static_follows_dependents = False,
        extra_linker_outputs_factory = _get_extra_linker_flags_and_outputs,
        swiftmodule_linkable = get_swiftmodule_linkable(swift_compile),
    )

def _get_extra_linker_flags_and_outputs(
        ctx: AnalysisContext) -> (list[ArgLike], dict[str, list[DefaultInfo]]):
    _ = ctx  # buildifier: disable=unused-variable
    # @oss-disable: return add_extra_linker_outputs(ctx) 
    return [], {} # @oss-enable

def _filter_swift_srcs(ctx: AnalysisContext) -> (list[CxxSrcWithFlags], list[CxxSrcWithFlags]):
    cxx_srcs = []
    swift_srcs = []
    for s in get_srcs_with_flags(ctx):
        if s.file.extension == SWIFT_EXTENSION:
            swift_srcs.append(s)
        else:
            cxx_srcs.append(s)

    return cxx_srcs, swift_srcs

def _get_link_style_sub_targets_and_providers(
        output_style: LibOutputStyle,
        ctx: AnalysisContext,
        output: [CxxLibraryOutput, None]) -> (dict[str, list[Provider]], list[Provider]):
    # We always propagate a resource graph regardless of link style or empty output
    resource_graph = create_resource_graph(
        ctx = ctx,
        labels = ctx.attrs.labels,
        deps = cxx_attr_deps(ctx),
        exported_deps = cxx_attr_exported_deps(ctx),
        # Shared libraries should not propagate their resources to rdeps,
        # they should only be contained in their frameworks apple_bundle.
        should_propagate = output_style != LibOutputStyle("shared_lib"),
    )
    if output_style != LibOutputStyle("shared_lib") or output == None:
        return ({}, [resource_graph])

    min_version = get_min_deployment_version_for_node(ctx)
    min_version_providers = [AppleMinDeploymentVersionInfo(version = min_version)]

    debug_info = project_artifacts(
        actions = ctx.actions,
        tsets = [output.external_debug_info],
    )

    if get_apple_stripped_attr_value_with_default_fallback(ctx):
        if False:
            # TODO(nga): `output.unstripped` is never `None`.
            def unknown():
                pass

            output = unknown()
        expect(output.unstripped != None, "Expecting unstripped output to be non-null when stripping is enabled.")
        dsym_executable = output.unstripped
    else:
        dsym_executable = output.default
    dsym_artifact = get_apple_dsym(
        ctx = ctx,
        executable = dsym_executable,
        debug_info = debug_info,
        action_identifier = dsym_executable.short_path,
    )
    debug_info_artifacts_manifest = ctx.actions.write(
        "debuginfo.artifacts",
        debug_info,
        with_inputs = True,
    )
    subtargets = {
        DSYM_SUBTARGET: [DefaultInfo(default_output = dsym_artifact)],
        DEBUGINFO_SUBTARGET: [DefaultInfo(default_output = debug_info_artifacts_manifest)],
    }
    providers = [
        AppleDebuggableInfo(dsyms = [dsym_artifact], debug_info_tset = output.external_debug_info),
        resource_graph,
    ] + min_version_providers

    if output.linker_map != None:
        subtargets["linker-map"] = [DefaultInfo(default_output = output.linker_map.map, other_outputs = [output.linker_map.binary])]
        providers += [AppleBundleLinkerMapInfo(linker_maps = [output.linker_map.map])]

    return (subtargets, providers)

def _get_linker_flags(ctx: AnalysisContext) -> cmd_args:
    return cmd_args(get_min_deployment_version_target_linker_flags(ctx))

def _xcode_populate_attributes(
        ctx,
        srcs: list[CxxSrcWithFlags],
        argsfiles: dict[str, CompileArgsfile],
        populate_xcode_attributes_func: typing.Callable,
        **_kwargs) -> dict[str, typing.Any]:
    # Overwrite the product name
    data = populate_xcode_attributes_func(ctx, srcs = srcs, argsfiles = argsfiles, product_name = ctx.attrs.name)
    return data
