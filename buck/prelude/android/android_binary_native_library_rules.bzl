# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//:paths.bzl", "paths")
load(
    "@prelude//android:android_providers.bzl",
    "AndroidBinaryNativeLibsInfo",
    "AndroidPackageableInfo",
    "ExopackageNativeInfo",
    "PrebuiltNativeLibraryDir",  # @unused Used as a type
)
load("@prelude//android:android_toolchain.bzl", "AndroidToolchainInfo")
load("@prelude//android:cpu_filters.bzl", "CPU_FILTER_FOR_PRIMARY_PLATFORM", "CPU_FILTER_TO_ABI_DIRECTORY")
load("@prelude//android:util.bzl", "EnhancementContext")
load("@prelude//android:voltron.bzl", "ROOT_MODULE", "all_targets_in_root_module", "get_apk_module_graph_info", "is_root_module")
load("@prelude//cxx:cxx_toolchain_types.bzl", "CxxToolchainInfo")
load(
    "@prelude//cxx:link.bzl",
    "cxx_link_shared_library",
)
load("@prelude//cxx:link_types.bzl", "link_options")
load(
    "@prelude//cxx:symbols.bzl",
    "extract_global_syms",
    "extract_undefined_syms",
)
load("@prelude//java:java_library.bzl", "compile_to_jar")  # @unused
load("@prelude//java:java_providers.bzl", "JavaClasspathEntry", "JavaLibraryInfo", "derive_compiling_deps")  # @unused
load("@prelude//linking:execution_preference.bzl", "LinkExecutionPreference")
load(
    "@prelude//linking:link_info.bzl",
    "LibOutputStyle",
    "LinkArgs",
    "LinkInfo",
    "Linkage",
    "SharedLibLinkable",
    "set_link_info_link_whole",
    "wrap_link_info",
)
load(
    "@prelude//linking:linkable_graph.bzl",
    "LinkableGraph",  # @unused Used as a type
    "LinkableNode",  # @unused Used as a type
    "create_linkable_graph",
    "get_linkable_graph_node_map_func",
)
load(
    "@prelude//linking:shared_libraries.bzl",
    "SharedLibrary",  # @unused Used as a type
    "SharedLibraryInfo",  # @unused Used as a type
    "get_strip_non_global_flags",
    "merge_shared_libraries",
    "traverse_shared_library_info",
)
load("@prelude//linking:strip.bzl", "strip_object")
load("@prelude//utils:graph_utils.bzl", "breadth_first_traversal_by", "post_order_traversal", "pre_order_traversal", "pre_order_traversal_by")
load("@prelude//utils:set.bzl", "set", "set_type")  # @unused Used as a type
load("@prelude//utils:utils.bzl", "dedupe_by_value", "expect")

# Native libraries on Android are built for a particular Application Binary Interface (ABI). We
# package native libraries for one (or more, for multi-arch builds) ABIs into an Android APK.
#
# Our native libraries come from two sources:
# 1. "Prebuilt native library dirs", which are directory artifacts whose sub-directories are ABIs,
#    and those ABI subdirectories contain native libraries. These come from `android_prebuilt_aar`s
#    and `prebuilt_native_library`s, for example.
# 2. "Native linkables". These are each a single shared library - `.so`s for one particular ABI.
#
# Native libraries can be packaged into Android APKs in two ways.
# 1. As native libraries. This means that they are passed to the APK builder as native libraries,
#    and the APK builder will package `<ABI>/library.so` into the APK at `libs/<ABI>/library.so`.
# 2. As assets. These are passed to the APK build as assets, and are stored at
#    `assets/lib/<ABI>/library.so` In the root module, we only package a native library as an
#    asset if it is eligible to be an asset (e.g. `can_be_asset` on a `cxx_library`), and
#    `package_asset_libraries` is set to True for the APK. We will additionally compress all the
#    assets into a single `assets/lib/libs.xz` (or `assets/libs/libs.zstd` for `zstd` compression)
#    if `compress_asset_libraries` is set to True for the APK. Regardless of whether we compress
#    the assets or not, we create a metadata file at `assets/libs/metadata.txt` that has a single
#    line entry for each packaged asset consisting of '<ABI/library_name> <file_size> <sha256>'.
#
#    Any native library that is not part of the root module (i.e. it is part of some other Voltron
#    module) is automatically packaged as an asset, and the assets for each module are compressed
#    to a single `assets/<module_name>/libs.xz`. Similarly, the metadata for each module is stored
#    at `assets/<module_name>/libs.txt`.

def get_android_binary_native_library_info(
        enhance_ctx: EnhancementContext,
        android_packageable_info: AndroidPackageableInfo,
        deps_by_platform: dict[str, list[Dependency]],
        apk_module_graph_file: [Artifact, None] = None,
        prebuilt_native_library_dirs_to_exclude: [set_type, None] = None,
        shared_libraries_to_exclude: [set_type, None] = None) -> AndroidBinaryNativeLibsInfo:
    ctx = enhance_ctx.ctx

    traversed_prebuilt_native_library_dirs = android_packageable_info.prebuilt_native_library_dirs.traverse() if android_packageable_info.prebuilt_native_library_dirs else []
    all_prebuilt_native_library_dirs = [
        native_lib
        for native_lib in traversed_prebuilt_native_library_dirs
        if not (prebuilt_native_library_dirs_to_exclude and prebuilt_native_library_dirs_to_exclude.contains(native_lib.raw_target))
    ]

    included_shared_lib_targets = []
    platform_to_original_native_linkables = {}
    for platform, deps in deps_by_platform.items():
        if platform == CPU_FILTER_FOR_PRIMARY_PLATFORM and platform not in ctx.attrs.cpu_filters:
            continue

        native_linkables = get_native_linkables_by_default(ctx, platform, deps, shared_libraries_to_exclude)
        included_shared_lib_targets.extend([lib.label.raw_target() for lib in native_linkables.values()])
        platform_to_original_native_linkables[platform] = native_linkables

    native_libs = ctx.actions.declare_output("native_libs_symlink")
    native_libs_metadata = ctx.actions.declare_output("native_libs_metadata_symlink")
    native_libs_always_in_primary_apk = ctx.actions.declare_output("native_libs_always_in_primary_apk_symlink")
    native_lib_assets_for_primary_apk = ctx.actions.declare_output("native_lib_assets_for_primary_apk_symlink")
    stripped_native_linkable_assets_for_primary_apk = ctx.actions.declare_output("stripped_native_linkable_assets_for_primary_apk_symlink")
    root_module_metadata_assets = ctx.actions.declare_output("root_module_metadata_assets_symlink")
    root_module_compressed_lib_assets = ctx.actions.declare_output("root_module_compressed_lib_assets_symlink")
    non_root_module_metadata_assets = ctx.actions.declare_output("non_root_module_metadata_assets_symlink")
    non_root_module_compressed_lib_assets = ctx.actions.declare_output("non_root_module_compressed_lib_assets_symlink")

    unstripped_native_libraries = ctx.actions.declare_output("unstripped_native_libraries")
    unstripped_native_libraries_json = ctx.actions.declare_output("unstripped_native_libraries_json")
    unstripped_native_libraries_files = ctx.actions.declare_output("unstripped_native_libraries.links", dir = True)

    dynamic_outputs = [
        native_libs,
        native_libs_metadata,
        native_libs_always_in_primary_apk,
        native_lib_assets_for_primary_apk,
        unstripped_native_libraries,
        unstripped_native_libraries_json,
        unstripped_native_libraries_files,
        stripped_native_linkable_assets_for_primary_apk,
        root_module_metadata_assets,
        root_module_compressed_lib_assets,
        non_root_module_metadata_assets,
        non_root_module_compressed_lib_assets,
    ]

    fake_input = ctx.actions.write("dynamic.trigger", "")

    # some cases don't actually need to use a dynamic_output, but it's simplest to consistently use it anyway. we need some fake input to allow that.
    dynamic_inputs = [fake_input]
    if apk_module_graph_file:
        dynamic_inputs.append(apk_module_graph_file)
    native_library_merge_map = None
    native_library_merge_dir = None
    native_merge_debug = None
    generated_java_code = []

    glue_linkables = None
    if getattr(ctx.attrs, "native_library_merge_glue", None):
        glue_linkables = {}
        for platform, glue in ctx.attrs.native_library_merge_glue.items():
            glue_link_graph = glue.get(LinkableGraph)
            expect(glue_link_graph != None, "native_library_merge_glue (`{}`) should be a linkable target", glue.label)
            glue_linkable = glue_link_graph.nodes.value.linkable
            expect(glue_linkable != None, "native_library_merge_glue (`{}`) should be a linkable target", glue.label)
            expect(glue_linkable.preferred_linkage == Linkage("static"), "buck2 currently only supports preferred_linkage='static' native_library_merge_glue")
            glue_linkables[platform] = (glue.label, glue_linkable.link_infos[LibOutputStyle("pic_archive")].default)

    flattened_linkable_graphs_by_platform = {}
    native_library_merge_sequence = getattr(ctx.attrs, "native_library_merge_sequence", None)
    has_native_merging = native_library_merge_sequence or getattr(ctx.attrs, "native_library_merge_map", None)

    if has_native_merging:
        native_merge_debug = ctx.actions.declare_output("native_merge.debug")
        dynamic_outputs.append(native_merge_debug)

        # We serialize info about the linkable graph and the apk module mapping and pass that to an
        # external subcommand to apply a merge sequence algorithm and return us the merge mapping.
        for platform, deps in deps_by_platform.items():
            linkable_graph = create_linkable_graph(ctx, deps = deps)
            graph_node_map = get_linkable_graph_node_map_func(linkable_graph)()
            linkables_debug = ctx.actions.write("linkables." + platform, list(graph_node_map.keys()))
            enhance_ctx.debug_output("linkables." + platform, linkables_debug)

            flattened_linkable_graphs_by_platform[platform] = graph_node_map

    if native_library_merge_sequence:
        native_library_merge_input_file = ctx.actions.write_json("mergemap.input", {
            "linkable_graphs_by_platform": encode_linkable_graph_for_mergemap(flattened_linkable_graphs_by_platform),
            "native_library_merge_sequence": ctx.attrs.native_library_merge_sequence,
            "native_library_merge_sequence_blocklist": ctx.attrs.native_library_merge_sequence_blocklist,
        })
        mergemap_cmd = cmd_args(ctx.attrs._android_toolchain[AndroidToolchainInfo].mergemap_tool)
        mergemap_cmd.add(cmd_args(native_library_merge_input_file, format = "--mergemap-input={}"))
        if apk_module_graph_file:
            mergemap_cmd.add(cmd_args(apk_module_graph_file, format = "--apk-module-graph={}"))
        native_library_merge_dir = ctx.actions.declare_output("merge_sequence_output")
        native_library_merge_map = native_library_merge_dir.project("merge.map")
        mergemap_cmd.add(cmd_args(native_library_merge_dir.as_output(), format = "--output={}"))
        ctx.actions.run(mergemap_cmd, category = "compute_mergemap")
        enhance_ctx.debug_output("compute_merge_sequence", native_library_merge_dir)

        dynamic_inputs.append(native_library_merge_map)

    mergemap_gencode_jar = None
    if has_native_merging and ctx.attrs.native_library_merge_code_generator:
        mergemap_gencode_jar = ctx.actions.declare_output("MergedLibraryMapping.jar")
        dynamic_outputs.append(mergemap_gencode_jar)
        library_output = JavaClasspathEntry(
            full_library = mergemap_gencode_jar,
            abi = mergemap_gencode_jar,
            abi_as_dir = None,
            required_for_source_only_abi = False,
        )
        generated_java_code.append(
            JavaLibraryInfo(
                compiling_deps = derive_compiling_deps(ctx.actions, library_output, []),
                library_output = library_output,
                output_for_classpath_macro = library_output.full_library,
            ),
        )

    def dynamic_native_libs_info(ctx: AnalysisContext, artifacts, outputs):
        get_module_from_target = all_targets_in_root_module
        if apk_module_graph_file:
            get_module_from_target = get_apk_module_graph_info(ctx, apk_module_graph_file, artifacts).target_to_module_mapping_function

        if has_native_merging:
            native_library_merge_debug_outputs = {}

            # When changing this dynamic_output, the workflow is a lot better if you compute the module graph once and
            # then set it as the binary's precomputed_apk_module_graph attr.
            if ctx.attrs.native_library_merge_sequence:
                merge_map_by_platform = artifacts[native_library_merge_map].read_json()
                native_library_merge_debug_outputs["merge_sequence_output"] = native_library_merge_dir
            elif ctx.attrs.native_library_merge_map:
                merge_map_by_platform = {}
                for platform, linkable_nodes in flattened_linkable_graphs_by_platform.items():
                    merge_map = merge_map_by_platform.setdefault(platform, {})
                    for target, _node in linkable_nodes.items():
                        raw_target = str(target.raw_target())
                        merge_result = None
                        for merge_lib, patterns in ctx.attrs.native_library_merge_map.items():
                            for pattern in patterns:
                                if regex(pattern, fancy = True).match(raw_target):
                                    merge_result = merge_lib
                                    break
                            if merge_result:
                                break
                        if merge_result:
                            merge_map[str(target)] = merge_result
                merge_map = ctx.actions.write_json("merge.map", merge_map_by_platform)
                native_library_merge_debug_outputs["merge_map_output"] = merge_map
            else:
                fail("unreachable")

            merged_linkables = _get_merged_linkables(
                ctx,
                {
                    platform: LinkableMergeData(
                        glue_linkable = glue_linkables[platform] if glue_linkables else None,
                        default_shared_libs = platform_to_original_native_linkables[platform],
                        linkable_nodes = flattened_linkable_graphs_by_platform[platform],
                        merge_map = merge_map_by_platform[platform],
                        apk_module_graph = get_module_from_target,
                    )
                    for platform in platform_to_original_native_linkables
                },
            )
            debug_data_json = ctx.actions.write_json("native_merge_debug.json", merged_linkables.debug_info)
            native_library_merge_debug_outputs["native_merge_debug.json"] = debug_data_json
            if mergemap_gencode_jar:
                merged_library_map = write_merged_library_map(ctx, merged_linkables)
                mergemap_gencode = run_mergemap_codegen(ctx, merged_library_map)
                compile_to_jar(ctx, [mergemap_gencode], output = outputs[mergemap_gencode_jar])
                native_library_merge_debug_outputs["NativeLibraryMergeGeneratedCode.java"] = mergemap_gencode
                native_library_merge_debug_outputs["merged_library_map.txt"] = merged_library_map
                native_library_merge_debug_outputs["mergemap_gencode.jar"] = mergemap_gencode_jar

            ctx.actions.symlinked_dir(outputs[native_merge_debug], native_library_merge_debug_outputs)

            final_platform_to_native_linkables = {
                platform: {soname: d.lib for soname, d in merged_shared_libs.items()}
                for platform, merged_shared_libs in merged_linkables.shared_libs_by_platform.items()
            }
        else:
            final_platform_to_native_linkables = platform_to_original_native_linkables

        if getattr(ctx.attrs, "enable_relinker", False):
            final_platform_to_native_linkables = relink_libraries(ctx, final_platform_to_native_linkables)

        unstripped_libs = {}
        for platform, libs in final_platform_to_native_linkables.items():
            for lib in libs.values():
                unstripped_libs[lib.lib.output] = platform
        ctx.actions.write(outputs[unstripped_native_libraries], unstripped_libs.keys())
        ctx.actions.write_json(outputs[unstripped_native_libraries_json], unstripped_libs)
        ctx.actions.symlinked_dir(outputs[unstripped_native_libraries_files], {
            "{}/{}".format(platform, lib.short_path): lib
            for lib, platform in unstripped_libs.items()
        })

        dynamic_info = _get_native_libs_and_assets(
            ctx,
            get_module_from_target,
            all_prebuilt_native_library_dirs,
            final_platform_to_native_linkables,
        )

        # Since we are using a dynamic action, we need to declare the outputs in advance.
        # Rather than passing the created outputs into `_get_native_libs_and_assets`, we
        # just symlink to the outputs that function produces.
        ctx.actions.symlink_file(outputs[native_libs], dynamic_info.native_libs)
        ctx.actions.symlink_file(outputs[native_libs_metadata], dynamic_info.native_libs_metadata)
        ctx.actions.symlink_file(outputs[native_libs_always_in_primary_apk], dynamic_info.native_libs_always_in_primary_apk)
        ctx.actions.symlink_file(outputs[native_lib_assets_for_primary_apk], dynamic_info.native_lib_assets_for_primary_apk if dynamic_info.native_lib_assets_for_primary_apk else ctx.actions.symlinked_dir("empty_native_lib_assets", {}))
        ctx.actions.symlink_file(outputs[stripped_native_linkable_assets_for_primary_apk], dynamic_info.stripped_native_linkable_assets_for_primary_apk if dynamic_info.stripped_native_linkable_assets_for_primary_apk else ctx.actions.symlinked_dir("empty_stripped_native_linkable_assets", {}))
        ctx.actions.symlink_file(outputs[root_module_metadata_assets], dynamic_info.root_module_metadata_assets)
        ctx.actions.symlink_file(outputs[root_module_compressed_lib_assets], dynamic_info.root_module_compressed_lib_assets)
        ctx.actions.symlink_file(outputs[non_root_module_metadata_assets], dynamic_info.non_root_module_metadata_assets)
        ctx.actions.symlink_file(outputs[non_root_module_compressed_lib_assets], dynamic_info.non_root_module_compressed_lib_assets)

    ctx.actions.dynamic_output(dynamic = dynamic_inputs, inputs = [], outputs = dynamic_outputs, f = dynamic_native_libs_info)
    all_native_libs = ctx.actions.symlinked_dir("debug_all_native_libs", {"others": native_libs, "primary": native_libs_always_in_primary_apk})

    enhance_ctx.debug_output("debug_native_libs", all_native_libs)
    if native_merge_debug:
        enhance_ctx.debug_output("native_merge_debug", native_merge_debug)

    enhance_ctx.debug_output("unstripped_native_libraries", unstripped_native_libraries, other_outputs = [unstripped_native_libraries_files])
    enhance_ctx.debug_output("unstripped_native_libraries_json", unstripped_native_libraries_json, other_outputs = [unstripped_native_libraries_files])

    native_libs_for_primary_apk, exopackage_info = _get_exopackage_info(ctx, native_libs_always_in_primary_apk, native_libs, native_libs_metadata)
    return AndroidBinaryNativeLibsInfo(
        apk_under_test_prebuilt_native_library_dirs = all_prebuilt_native_library_dirs,
        apk_under_test_shared_libraries = included_shared_lib_targets,
        native_libs_for_primary_apk = native_libs_for_primary_apk,
        exopackage_info = exopackage_info,
        root_module_native_lib_assets = [native_lib_assets_for_primary_apk, stripped_native_linkable_assets_for_primary_apk, root_module_metadata_assets, root_module_compressed_lib_assets],
        non_root_module_native_lib_assets = [non_root_module_metadata_assets, non_root_module_compressed_lib_assets],
        generated_java_code = generated_java_code,
    )

# We could just return two artifacts of libs (one for the primary APK, one which can go
# either into the primary APK or be exopackaged), and one artifact of assets,
# but we'd need an extra action in order to combine them (we can't use `symlinked_dir` since
# the paths overlap) so it's easier to just be explicit about exactly what we produce.
_NativeLibsAndAssetsInfo = record(
    native_libs = Artifact,
    native_libs_metadata = Artifact,
    native_libs_always_in_primary_apk = Artifact,
    native_lib_assets_for_primary_apk = [Artifact, None],
    stripped_native_linkable_assets_for_primary_apk = [Artifact, None],
    root_module_metadata_assets = Artifact,
    root_module_compressed_lib_assets = Artifact,
    non_root_module_metadata_assets = Artifact,
    non_root_module_compressed_lib_assets = Artifact,
)

def _get_exopackage_info(
        ctx: AnalysisContext,
        native_libs_always_in_primary_apk: Artifact,
        native_libs: Artifact,
        native_libs_metadata: Artifact) -> (list[Artifact], [ExopackageNativeInfo, None]):
    is_exopackage_enabled_for_native_libs = "native_library" in getattr(ctx.attrs, "exopackage_modes", [])
    if is_exopackage_enabled_for_native_libs:
        return [native_libs_always_in_primary_apk], ExopackageNativeInfo(directory = native_libs, metadata = native_libs_metadata)
    else:
        return [native_libs, native_libs_always_in_primary_apk], None

def _get_native_libs_and_assets(
        ctx: AnalysisContext,
        get_module_from_target: typing.Callable,
        all_prebuilt_native_library_dirs: list[PrebuiltNativeLibraryDir],
        platform_to_native_linkables: dict[str, dict[str, SharedLibrary]]) -> _NativeLibsAndAssetsInfo:
    is_packaging_native_libs_as_assets_supported = getattr(ctx.attrs, "package_asset_libraries", False)

    prebuilt_native_library_dirs = []
    prebuilt_native_library_dirs_always_in_primary_apk = []
    prebuilt_native_library_dir_assets_for_primary_apk = []
    prebuilt_native_library_dir_module_assets_map = {}
    for native_lib in all_prebuilt_native_library_dirs:
        native_lib_target = str(native_lib.raw_target)
        module = get_module_from_target(native_lib_target)
        expect(
            not native_lib.for_primary_apk or is_root_module(module),
            "{} which is marked as needing to be in the primary APK cannot be included in non-root-module {}".format(native_lib_target, module),
        )
        expect(
            not native_lib.for_primary_apk or not native_lib.is_asset,
            "{} which is marked as needing to be in the primary APK cannot be an asset".format(native_lib_target),
        )
        if not is_root_module(module):
            if native_lib.is_asset:
                prebuilt_native_library_dir_module_assets_map.setdefault(module, []).append(native_lib)
            else:
                prebuilt_native_library_dirs.append(native_lib)
        elif native_lib.is_asset and is_packaging_native_libs_as_assets_supported:
            expect(not native_lib.for_primary_apk, "{} which is marked as needing to be in the primary APK cannot be an asset".format(native_lib_target))
            prebuilt_native_library_dir_assets_for_primary_apk.append(native_lib)
        elif native_lib.for_primary_apk:
            prebuilt_native_library_dirs_always_in_primary_apk.append(native_lib)
        else:
            prebuilt_native_library_dirs.append(native_lib)

    native_libs = _filter_prebuilt_native_library_dir(
        ctx,
        prebuilt_native_library_dirs,
        "native_libs",
    )
    native_libs_always_in_primary_apk = _filter_prebuilt_native_library_dir(
        ctx,
        prebuilt_native_library_dirs_always_in_primary_apk,
        "native_libs_always_in_primary_apk",
    )
    native_lib_assets_for_primary_apk = _filter_prebuilt_native_library_dir(
        ctx,
        prebuilt_native_library_dir_assets_for_primary_apk,
        "native_lib_assets_for_primary_apk",
        package_as_assets = True,
        module = ROOT_MODULE,
    ) if prebuilt_native_library_dir_assets_for_primary_apk else None
    native_lib_module_assets_map = {}
    for module, native_lib_dir in prebuilt_native_library_dir_module_assets_map.items():
        native_lib_module_assets_map[module] = [_filter_prebuilt_native_library_dir(
            ctx,
            native_lib_dir,
            "native_lib_assets_for_module_{}".format(module),
            package_as_assets = True,
            module = module,
        )]

    stripped_linkables = _get_native_linkables(ctx, platform_to_native_linkables, get_module_from_target, is_packaging_native_libs_as_assets_supported)
    for module, native_linkable_assets in stripped_linkables.linkable_module_assets_map.items():
        native_lib_module_assets_map.setdefault(module, []).append(native_linkable_assets)

    root_module_metadata_srcs = {}
    root_module_compressed_lib_srcs = {}
    non_root_module_metadata_srcs = {}
    non_root_module_compressed_lib_srcs = {}
    assets_for_primary_apk = filter(None, [native_lib_assets_for_primary_apk, stripped_linkables.linkable_assets_for_primary_apk])
    stripped_linkable_assets_for_primary_apk = stripped_linkables.linkable_assets_for_primary_apk
    if assets_for_primary_apk:
        metadata_file, native_library_paths = _get_native_libs_as_assets_metadata(ctx, assets_for_primary_apk, ROOT_MODULE)
        root_module_metadata_srcs[paths.join(_get_native_libs_as_assets_dir(ROOT_MODULE), "metadata.txt")] = metadata_file
        if ctx.attrs.compress_asset_libraries:
            compressed_lib_dir = _get_compressed_native_libs_as_assets(ctx, assets_for_primary_apk, native_library_paths, ROOT_MODULE)
            root_module_compressed_lib_srcs[_get_native_libs_as_assets_dir(ROOT_MODULE)] = compressed_lib_dir

            # Since we're storing these as compressed assets, we need to ignore the uncompressed libs.
            native_lib_assets_for_primary_apk = None
            stripped_linkable_assets_for_primary_apk = None

    for module, native_lib_assets in native_lib_module_assets_map.items():
        metadata_file, native_library_paths = _get_native_libs_as_assets_metadata(ctx, native_lib_assets, module)
        non_root_module_metadata_srcs[paths.join(_get_native_libs_as_assets_dir(module), "libs.txt")] = metadata_file
        compressed_lib_dir = _get_compressed_native_libs_as_assets(ctx, native_lib_assets, native_library_paths, module)
        non_root_module_compressed_lib_srcs[_get_native_libs_as_assets_dir(module)] = compressed_lib_dir

    combined_native_libs = ctx.actions.declare_output("combined_native_libs", dir = True)
    native_libs_metadata = ctx.actions.declare_output("native_libs_metadata.txt")
    ctx.actions.run(cmd_args([
        ctx.attrs._android_toolchain[AndroidToolchainInfo].combine_native_library_dirs[RunInfo],
        "--output-dir",
        combined_native_libs.as_output(),
        "--library-dirs",
        native_libs,
        stripped_linkables.linkables,
        "--metadata-file",
        native_libs_metadata.as_output(),
    ]), category = "combine_native_libs")

    combined_native_libs_always_in_primary_apk = ctx.actions.declare_output("combined_native_libs_always_in_primary_apk", dir = True)
    ctx.actions.run(cmd_args([
        ctx.attrs._android_toolchain[AndroidToolchainInfo].combine_native_library_dirs[RunInfo],
        "--output-dir",
        combined_native_libs_always_in_primary_apk.as_output(),
        "--library-dirs",
        native_libs_always_in_primary_apk,
        stripped_linkables.linkables_always_in_primary_apk,
    ]), category = "combine_native_libs_always_in_primary_apk")

    return _NativeLibsAndAssetsInfo(
        native_libs = combined_native_libs,
        native_libs_metadata = native_libs_metadata,
        native_libs_always_in_primary_apk = combined_native_libs_always_in_primary_apk,
        native_lib_assets_for_primary_apk = native_lib_assets_for_primary_apk,
        stripped_native_linkable_assets_for_primary_apk = stripped_linkable_assets_for_primary_apk,
        root_module_metadata_assets = ctx.actions.symlinked_dir("root_module_metadata_assets", root_module_metadata_srcs),
        root_module_compressed_lib_assets = ctx.actions.symlinked_dir("root_module_compressed_lib_assets", root_module_compressed_lib_srcs),
        non_root_module_metadata_assets = ctx.actions.symlinked_dir("non_root_module_metadata_assets", non_root_module_metadata_srcs),
        non_root_module_compressed_lib_assets = ctx.actions.symlinked_dir("non_root_module_compressed_lib_assets", non_root_module_compressed_lib_srcs),
    )

def _filter_prebuilt_native_library_dir(
        ctx: AnalysisContext,
        native_libs: list[PrebuiltNativeLibraryDir],
        identifier: str,
        package_as_assets: bool = False,
        module: str = ROOT_MODULE) -> Artifact:
    cpu_filters = ctx.attrs.cpu_filters or CPU_FILTER_TO_ABI_DIRECTORY.keys()
    abis = [CPU_FILTER_TO_ABI_DIRECTORY[cpu] for cpu in cpu_filters]
    filter_tool = ctx.attrs._android_toolchain[AndroidToolchainInfo].filter_prebuilt_native_library_dir[RunInfo]
    native_libs_dirs = [native_lib.dir for native_lib in native_libs]
    native_libs_dirs_file = ctx.actions.write("{}_list.txt".format(identifier), native_libs_dirs)
    base_output_dir = ctx.actions.declare_output(identifier, dir = True)
    output_dir = base_output_dir.project(_get_native_libs_as_assets_dir(module)) if package_as_assets else base_output_dir
    ctx.actions.run(
        cmd_args([filter_tool, native_libs_dirs_file, output_dir.as_output(), "--abis"] + abis).hidden(native_libs_dirs),
        category = "filter_prebuilt_native_library_dir",
        identifier = identifier,
    )

    return base_output_dir

_StrippedNativeLinkables = record(
    linkables = Artifact,
    linkables_always_in_primary_apk = Artifact,
    linkable_assets_for_primary_apk = [Artifact, None],
    linkable_module_assets_map = dict[str, Artifact],
)

def _get_native_linkables(
        ctx: AnalysisContext,
        platform_to_native_linkables: dict[str, dict[str, SharedLibrary]],
        get_module_from_target: typing.Callable,
        package_native_libs_as_assets_enabled: bool) -> _StrippedNativeLinkables:
    stripped_native_linkables_srcs = {}
    stripped_native_linkables_always_in_primary_apk_srcs = {}
    stripped_native_linkable_assets_for_primary_apk_srcs = {}
    stripped_native_linkable_module_assets_srcs = {}

    cpu_filters = ctx.attrs.cpu_filters
    for platform, native_linkables in platform_to_native_linkables.items():
        if cpu_filters and platform not in cpu_filters and platform != CPU_FILTER_FOR_PRIMARY_PLATFORM:
            fail("Platform `{}` is not in the CPU filters `{}`".format(platform, cpu_filters))

        abi_directory = CPU_FILTER_TO_ABI_DIRECTORY[platform]
        for so_name, native_linkable in native_linkables.items():
            native_linkable_target = str(native_linkable.label.raw_target())
            module = get_module_from_target(native_linkable_target)

            expect(
                not native_linkable.for_primary_apk or is_root_module(module),
                "{} which is marked as needing to be in the primary APK cannot be included in non-root-module {}".format(native_linkable_target, module),
            )
            expect(
                not native_linkable.for_primary_apk or not native_linkable.can_be_asset,
                "{} which is marked as needing to be in the primary APK cannot be an asset".format(native_linkable_target),
            )
            if native_linkable.can_be_asset and not is_root_module(module):
                so_name_path = paths.join(_get_native_libs_as_assets_dir(module), abi_directory, so_name)
                stripped_native_linkable_module_assets_srcs.setdefault(module, {})[so_name_path] = native_linkable.stripped_lib
            elif native_linkable.can_be_asset and package_native_libs_as_assets_enabled:
                so_name_path = paths.join(_get_native_libs_as_assets_dir(module), abi_directory, so_name)
                stripped_native_linkable_assets_for_primary_apk_srcs[so_name_path] = native_linkable.stripped_lib
            else:
                so_name_path = paths.join(abi_directory, so_name)
                if native_linkable.for_primary_apk:
                    stripped_native_linkables_always_in_primary_apk_srcs[so_name_path] = native_linkable.stripped_lib
                else:
                    stripped_native_linkables_srcs[so_name_path] = native_linkable.stripped_lib

    stripped_native_linkables = ctx.actions.symlinked_dir(
        "stripped_native_linkables",
        stripped_native_linkables_srcs,
    )
    stripped_native_linkables_always_in_primary_apk = ctx.actions.symlinked_dir(
        "stripped_native_linkables_always_in_primary_apk",
        stripped_native_linkables_always_in_primary_apk_srcs,
    )
    stripped_native_linkable_assets_for_primary_apk = ctx.actions.symlinked_dir(
        "stripped_native_linkables_assets_for_primary_apk",
        stripped_native_linkable_assets_for_primary_apk_srcs,
    ) if stripped_native_linkable_assets_for_primary_apk_srcs else None
    stripped_native_linkable_module_assets_map = {}
    for module, srcs in stripped_native_linkable_module_assets_srcs.items():
        stripped_native_linkable_module_assets_map[module] = ctx.actions.symlinked_dir(
            "stripped_native_linkable_assets_for_module_{}".format(module),
            srcs,
        )

    return _StrippedNativeLinkables(
        linkables = stripped_native_linkables,
        linkables_always_in_primary_apk = stripped_native_linkables_always_in_primary_apk,
        linkable_assets_for_primary_apk = stripped_native_linkable_assets_for_primary_apk,
        linkable_module_assets_map = stripped_native_linkable_module_assets_map,
    )

def _get_native_libs_as_assets_metadata(
        ctx: AnalysisContext,
        native_lib_assets: list[Artifact],
        module: str) -> (Artifact, Artifact):
    native_lib_assets_file = ctx.actions.write("{}/native_lib_assets".format(module), [cmd_args([native_lib_asset, _get_native_libs_as_assets_dir(module)], delimiter = "/") for native_lib_asset in native_lib_assets])
    metadata_output = ctx.actions.declare_output("{}/native_libs_as_assets_metadata.txt".format(module))
    native_library_paths = ctx.actions.declare_output("{}/native_libs_as_assets_paths.txt".format(module))
    metadata_cmd = cmd_args([
        ctx.attrs._android_toolchain[AndroidToolchainInfo].native_libs_as_assets_metadata[RunInfo],
        "--native-library-dirs",
        native_lib_assets_file,
        "--metadata-output",
        metadata_output.as_output(),
        "--native-library-paths-output",
        native_library_paths.as_output(),
    ]).hidden(native_lib_assets)
    ctx.actions.run(metadata_cmd, category = "get_native_libs_as_assets_metadata", identifier = module)
    return metadata_output, native_library_paths

def _get_compressed_native_libs_as_assets(
        ctx: AnalysisContext,
        native_lib_assets: list[Artifact],
        native_library_paths: Artifact,
        module: str) -> Artifact:
    output_dir = ctx.actions.declare_output("{}/compressed_native_libs_as_assets_dir".format(module))
    compressed_libraries_cmd = cmd_args([
        ctx.attrs._android_toolchain[AndroidToolchainInfo].compress_libraries[RunInfo],
        "--libraries",
        native_library_paths,
        "--output-dir",
        output_dir.as_output(),
        "--compression-type",
        ctx.attrs.asset_compression_algorithm or "xz",
        "--xz-compression-level",
        str(ctx.attrs.xz_compression_level),
    ]).hidden(native_lib_assets)
    ctx.actions.run(compressed_libraries_cmd, category = "compress_native_libs_as_assets", identifier = module)
    return output_dir

def _get_native_libs_as_assets_dir(module: str) -> str:
    return "assets/{}".format("lib" if is_root_module(module) else module)

def get_native_linkables_by_default(ctx: AnalysisContext, _platform: str, deps: list[Dependency], shared_libraries_to_exclude) -> dict[str, SharedLibrary]:
    shared_library_info = merge_shared_libraries(
        ctx.actions,
        deps = filter(None, [x.get(SharedLibraryInfo) for x in deps]),
    )
    return {
        so_name: shared_lib
        for so_name, shared_lib in traverse_shared_library_info(shared_library_info).items()
        if not (shared_libraries_to_exclude and shared_libraries_to_exclude.contains(shared_lib.label.raw_target()))
    }

_LinkableSharedNode = record(
    raw_target = field(str),
    soname = field(str),
    labels = field(list[str], []),
    # Linkable deps of this target.
    deps = field(list[Label], []),
    can_be_asset = field(bool),
)

def encode_linkable_graph_for_mergemap(graph_node_map_by_platform: dict[str, dict[Label, LinkableNode]]) -> dict[str, dict[Label, _LinkableSharedNode]]:
    return {
        platform: {
            target: _LinkableSharedNode(
                raw_target = str(target.raw_target()),
                soname = node.default_soname,
                labels = node.labels,
                deps = node.deps + node.exported_deps,
                can_be_asset = node.can_be_asset,  # and not node.exclude_from_android_merge
            )
            for target, node in graph_node_map.items()
        }
        for platform, graph_node_map in graph_node_map_by_platform.items()
    }

# Debugging info about the linkables merge process. All of this will be written in one of the outputs of
# the `[native_merge_debug]` subtarget.
MergedLinkablesDebugInfo = record(
    unmerged_statics = list[str],
    group_debug = dict[str, typing.Any],
    with_default_soname = list[typing.Any],
    missing_default_solibs = list[Label],
)

# As shared lib output of the linkables merge process. This is not necessarily an actually merged node (there
# may be just one constituent)
MergedSharedLibrary = record(
    soname = str,
    lib = SharedLibrary,
    apk_module = str,
    # this only includes solib constituents that are included in the android merge map
    solib_constituents = list[str],
    is_actually_merged = bool,
)

# Output of the linkables merge process, the list of shared libs for each platform and
# debug information about the merge process itself.
MergedLinkables = record(
    # dict[platform, dict[final_soname, MergedSharedLibrary]]
    shared_libs_by_platform = dict[str, dict[str, MergedSharedLibrary]],
    debug_info = dict[str, MergedLinkablesDebugInfo],
)

# Input data to the linkables merge process
LinkableMergeData = record(
    glue_linkable = [(Label, LinkInfo), None],
    default_shared_libs = dict[str, SharedLibrary],
    linkable_nodes = dict[Label, LinkableNode],
    merge_map = dict[str, [str, None]],
    apk_module_graph = typing.Callable,
)

# information about a link group derived from the merge mapping
LinkGroupData = record(
    group_name = [str, Label],
    constituents = list[Label],
    apk_module = str,
)

# Represents a node in the final merged linkable map. Most of these will be shared libraries, either prebuilt shared libs or
# libraries that are created below for a node in the link_groups_graph. The exception is for non-merged static-only nodes, in
# that case this
LinkGroupLinkableNode = record(
    # The LinkInfo to add to the link line for a node that links against this.
    link = LinkInfo,
    deps = list[str],
    exported_deps = list[str],
    shared_lib = [SharedLibrary, None],

    # linker flags to be exported by any node that links against this. This can only be non-None for non-merged static only nodes (as we don't
    # propagate exported linker flags through transitive shared lib deps).
    exported_linker_flags = [(list[typing.Any], list[typing.Any]), None],
)

def write_merged_library_map(ctx: AnalysisContext, merged_linkables: MergedLinkables) -> Artifact:
    """
    Writes the "merged library map". This is a map of original soname to final soname of the form:

    ```
    original_soname1 final_soname1
    original_soname2 final_soname1
    original_soname3 final_soname2
    ...
    ```
    """
    solib_map = {}  # dict[final_soname, set[original_soname]]
    for _, shared_libs in merged_linkables.shared_libs_by_platform.items():
        for soname in shared_libs.keys():
            merged_shared_lib = shared_libs[soname]
            if merged_shared_lib.is_actually_merged:
                solib_map.setdefault(soname, set()).update(merged_shared_lib.solib_constituents)

    lines = []
    for final_soname in sorted(solib_map.keys()):
        for original_soname in solib_map[final_soname].list():
            lines.append("{} {}".format(original_soname, final_soname))

    # we wanted it sorted by original_soname
    return ctx.actions.write("merged_library_map.txt", sorted(lines))

def run_mergemap_codegen(ctx: AnalysisContext, merged_library_map: Artifact) -> Artifact:
    mapping_java = ctx.actions.declare_output("MergedLibraryMapping.java")
    args = cmd_args(ctx.attrs.native_library_merge_code_generator[RunInfo])
    args.add([merged_library_map, mapping_java.as_output()])
    ctx.actions.run(args, category = "mergemap_codegen")
    return mapping_java

def expect_dedupe(v):
    # asserts that the input list is unique
    o = dedupe_by_value(v)
    expect(len(o) == len(v), "expected `{}` to be a list of unique items, but it wasn't. deduped list was `{}`.", v, o)
    return v

def _get_merged_linkables(
        ctx: AnalysisContext,
        merged_data_by_platform: dict[str, LinkableMergeData]) -> MergedLinkables:
    """
    This takes the merge mapping and constructs the resulting merged shared libraries.

    This is similar to link_groups used by ios and cxx_binary, but is sufficiently different that it needs its
    own implementation. Potentially we could find a generalization of them that covers both.

    An overview of how this works:

    The input includes a mapping of target -> link group. This mapping does not need to be comprehensive, a target
    not in that mapping is assigned to its own link group. The input also contains a mapping of target -> apk module,
    and this must have a mapping for every target. A link group cannot contain targets in different apk modules.

    Once we have the mapping of link groups, we construct an analog of the LinkableGraph on the new link group graph.
    This new graph consists of LinkGroupLinkableNodes (in some sense, the merged equivalent of LinkableNode).
    We traverse this graph from the bottom up, producing link info for each node.

    First, there are some special cases:

    If a target is "not actually merged" (i.e. it's in a link group where it is the only constituent) then it may get
    special handling if:
        (1) it is lib with static preferred linkage or it contains no linkables (i.e. no code)
        (2) a prebuild shared lib
    For both these cases we will produce a LinkGroupLinkableNode that basically reuses their original information. For
    every other case we will produce a node that represents a shared library that we define.

    When constructing a LinkableNode for a link group, we will be traversing sort of a hybrid graph, as we will traverse
    the primary constituents of the link group itself in the original LinkableGraph, but dependencies outside of the link
    group will be mapped to the corresponding LinkGroupLinkableNode (and potentially then further traversing that node's
    deps).

    The merge mapping input determines the "primary constituents" of each link group. The "real constituents" of that link
    group will include all those primary constituents, all of the transitive non-shared lib deps in the linkgroup linkable
    graph, and then all the shared lib dependencies of all of them (that is describing a traversal similar to the
    normal link strategies).

    There are some subtle differences in the handling of primary constituents and the statically linked non-primary
    constituents:
    1. all primary constituents are public nodes, non primary ones are only public if they are transitively exported_deps
    of a primary constituent. A public node is linked via "link whole".
    2. linker_flags of primary constituents are included in the link, for non primary they are not
    """
    debug_info_by_platform = {}
    shared_libs_by_platform = {}
    for platform, merge_data in merged_data_by_platform.items():
        debug_info = debug_info_by_platform.setdefault(platform, MergedLinkablesDebugInfo(
            unmerged_statics = [],
            group_debug = {},
            with_default_soname = [],
            missing_default_solibs = [],
        ))
        linkable_nodes = merge_data.linkable_nodes

        linkable_nodes_graph = {k: dedupe(v.deps + v.exported_deps) for k, v in linkable_nodes.items()}
        topo_sorted_targets = pre_order_traversal(linkable_nodes_graph)

        # first we collect basic information about each link group, this will populate the fields in LinkGroupData and
        # map target labels to their link group name.
        link_groups = {}
        target_to_link_group = {}

        for target in topo_sorted_targets:
            expect(target not in target_to_link_group, "prelude internal error, target seen twice?")
            target_apk_module = merge_data.apk_module_graph(str(target.raw_target()))

            link_group = merge_data.merge_map.get(str(target), None)
            if not link_group:
                link_group = str(target)
                link_groups[link_group] = LinkGroupData(
                    group_name = target,
                    constituents = [target],
                    apk_module = target_apk_module,
                )
            elif link_group in link_groups:
                link_group_data = link_groups[link_group]

                # TODO(cjhopman): buck1 provides a more useful error here in that it lists the module mappings for all
                # constituents of the merge group (rather than just one conflict). That allows users to resolve all the
                # issues at once. With merge sequence merging (the replacement for merge map), this error shouldn't ever be hit
                # and so maybe it's not necessary to improve it.
                expect(
                    link_group_data.apk_module == target_apk_module,
                    "Native library merge of {} has inconsistent application module mappings:\n{} is in module {}\n{} is in module {}",
                    link_group_data.group_name,
                    target,
                    target_apk_module,
                    link_group_data.constituents[0],
                    link_group_data.apk_module,
                )
                link_groups[link_group].constituents.append(target)
            else:
                link_groups[link_group] = LinkGroupData(
                    group_name = link_group,
                    constituents = [target],
                    apk_module = target_apk_module,
                )

            target_to_link_group[target] = link_group

        # Now that all targets are assigned to a link group, build up the link group graph.
        link_groups_graph_builder = {}
        for target in topo_sorted_targets:
            target_group = target_to_link_group[target]
            group_deps = link_groups_graph_builder.setdefault(target_group, {})
            for dep in linkable_nodes_graph[target]:
                dep_group = target_to_link_group[dep]
                if target_group != dep_group:
                    group_deps[dep_group] = True
        link_groups_graph = {k: list(v.keys()) for k, v in link_groups_graph_builder.items()}

        archive_output_style = LibOutputStyle("pic_archive")
        shlib_output_style = LibOutputStyle("shared_lib")

        cxx_toolchain = ctx.attrs._cxx_toolchain[platform][CxxToolchainInfo]

        link_group_linkable_nodes = {}
        group_shared_libs = {}
        included_default_solibs = {}

        def platform_output_path(path):
            if len(merged_data_by_platform) > 1:
                return platform + "/" + path
            return path

        # Now we will traverse from the leaves up the graph (the link groups graph). As we traverse, we will produce
        # a link group linkablenode for each group.
        for group in post_order_traversal(link_groups_graph):
            group_data = link_groups[group]
            is_actually_merged = len(group_data.constituents) > 1
            can_be_asset = True

            if not is_actually_merged:
                target = group_data.constituents[0]
                node_data = linkable_nodes[target]
                can_be_asset = node_data.can_be_asset

                def has_linkable(node_data: LinkableNode) -> bool:
                    for _, output in node_data.link_infos.items():
                        if output.default.linkables:
                            return True
                    return False

                if node_data.preferred_linkage == Linkage("static") or not has_linkable(node_data):
                    debug_info.unmerged_statics.append(target)
                    link_group_linkable_nodes[group] = LinkGroupLinkableNode(
                        link = node_data.link_infos[archive_output_style].default,
                        deps = dedupe_by_value([target_to_link_group[t] for t in node_data.deps]),
                        exported_deps = dedupe_by_value([target_to_link_group[t] for t in node_data.exported_deps]),
                        shared_lib = None,
                        exported_linker_flags = (node_data.linker_flags.exported_flags, node_data.linker_flags.exported_post_flags),
                    )
                    continue

                # We can't merge a prebuilt shared (that has no archive) and must use it's original info.
                # Ideally this would probably be structured info on the linkablenode.
                def is_prebuilt_shared(node_data: LinkableNode) -> bool:
                    shared_link_info = node_data.link_infos.get(shlib_output_style, None)
                    if not shared_link_info or not shared_link_info.default.linkables:
                        return False
                    pic_archive_info = node_data.link_infos.get(archive_output_style, None)
                    if not pic_archive_info or not pic_archive_info.default.linkables:
                        return True
                    return False

                if is_prebuilt_shared(node_data):
                    expect(
                        len(node_data.shared_libs) == 1,
                        "unexpected shared_libs length for somerge of {} ({})".format(target, node_data.shared_libs),
                    )
                    expect(not node_data.deps, "prebuilt shared library `{}` with deps not supported by somerge".format(target))
                    expect(not node_data.exported_deps, "prebuilt shared library `{}` with exported_deps not supported by somerge".format(target))
                    soname, shlib = node_data.shared_libs.items()[0]

                    output_path = platform_output_path(shlib.output.short_path)
                    shared_lib = SharedLibrary(
                        lib = shlib,
                        stripped_lib = strip_lib(ctx, cxx_toolchain, shlib.output, output_path = output_path),
                        link_args = None,
                        shlib_deps = None,
                        can_be_asset = can_be_asset,
                        for_primary_apk = False,
                        soname = soname,
                        label = target,
                    )

                    link_group_linkable_nodes[group] = LinkGroupLinkableNode(
                        link = node_data.link_infos[shlib_output_style].default,
                        deps = [],
                        exported_deps = [],
                        shared_lib = shared_lib,
                        # exported linker flags for shared libs are in their linkinfo itself and are not exported from dependents
                        exported_linker_flags = None,
                    )
                    group_shared_libs[soname] = MergedSharedLibrary(
                        soname = soname,
                        lib = shared_lib,
                        apk_module = group_data.apk_module,
                        solib_constituents = [],
                        is_actually_merged = False,
                    )
                    continue

            # Keys in the current group stay as a Label, deps get converted to the group key.
            def convert_to_merged_graph_deps(deps: list[Label], curr_group: str) -> list[[Label, str]]:
                converted = []
                for dep in deps:
                    dep_group = target_to_link_group[dep]
                    if dep_group == curr_group:
                        converted.append(dep)
                    elif dep_group:
                        converted.append(dep_group)
                return dedupe_by_value(converted)

            # For the current group, this will traverse the original linkable graph to find the LinkableNodes for
            # the constituents of the group and traverses the link_group graph for non-constituent deps.
            def get_merged_graph_traversal(curr_group: str, exported_only: bool) -> typing.Callable:
                def traversal(key: [Label, str]) -> list[[Label, str]]:
                    if eval_type(Label).matches(key):
                        expect(target_to_link_group[key] == curr_group)
                        node = linkable_nodes[key]
                        if exported_only:
                            return convert_to_merged_graph_deps(node.exported_deps, curr_group)
                        return convert_to_merged_graph_deps(node.deps + node.exported_deps, curr_group)
                    else:
                        link_group_node = link_group_linkable_nodes[key]
                        if exported_only:
                            return link_group_node.exported_deps
                        return dedupe_by_value(link_group_node.deps + link_group_node.exported_deps)

                # It's easy for us to accidentally get this merged traversal wrong, so this provides one guardrail
                def checked_traversal(key: [Label, str]) -> list[[Label, str]]:
                    return expect_dedupe(traversal(key))

                return checked_traversal

            # note that this will possibly contain shared lib dependencies which aren't really public. that's handled below.
            public_node_roots = group_data.constituents

            # this is a hybrid of buck1 somerge behavior and what we do for link groups.
            # like link groups, we expose link group by setting link_whole on its link infos (this matches buck1 for
            # primary constituents, but not for other constituents).
            # like buck1, we treat all primary constituents as public node roots (as opposed to link groups that only treats
            # preferred_linkage=shared and edges with an outbound dep as public roots), and then traverse exported deps from
            # those roots to find all public nodes.
            # the main thing to note from this is that for non-primary constituents that are identified as public, we will
            # use link_whole whereas buck1 will make dependents link against them directly
            exported_public_nodes = {
                d: True
                for d in breadth_first_traversal_by(
                    None,
                    public_node_roots,
                    get_merged_graph_traversal(group, True),
                )
            }

            exported_linker_flags = []
            exported_linker_post_flags = []
            links = []
            shared_lib_deps = []
            real_constituents = []

            if is_actually_merged and merge_data.glue_linkable:
                real_constituents.append(merge_data.glue_linkable[0])
                links.append(set_link_info_link_whole(merge_data.glue_linkable[1]))

            solib_constituents = []
            link_group_deps = []
            ordered_group_constituents = pre_order_traversal_by(group_data.constituents, get_merged_graph_traversal(group, False))
            representative_label = ordered_group_constituents[0]
            for key in ordered_group_constituents:
                real_constituents.append(key)
                if eval_type(Label).matches(key):
                    # This is handling targets within this link group
                    expect(target_to_link_group[key] == group)
                    node = linkable_nodes[key]

                    default_solibs = list(node.shared_libs.keys())
                    if not default_solibs and node.preferred_linkage == Linkage("static"):
                        default_solibs = [node.default_soname]

                    for soname in default_solibs:
                        included_default_solibs[soname] = True
                        if node.include_in_android_mergemap:
                            solib_constituents.append(soname)

                    node = linkable_nodes[key]
                    link_info = node.link_infos[archive_output_style].default

                    # the propagated link info should already be wrapped with exported flags.
                    link_info = wrap_link_info(
                        link_info,
                        pre_flags = node.linker_flags.flags,
                        post_flags = node.linker_flags.post_flags,
                    )
                    exported_linker_flags.extend(node.linker_flags.exported_flags)
                    exported_linker_post_flags.extend(node.linker_flags.exported_post_flags)
                    if key in exported_public_nodes:
                        link_info = set_link_info_link_whole(link_info)
                else:
                    # This is cross-link-group deps. We add information to the link line from the LinkGroupLinkableNode of the dep.
                    link_group_node = link_group_linkable_nodes[key]
                    link_info = link_group_node.link
                    if link_group_node.shared_lib:
                        shared_lib_deps.append(link_group_node.shared_lib.soname)
                        link_group_deps.append(key)
                    elif key in exported_public_nodes:
                        link_info = set_link_info_link_whole(link_info)

                        if link_group_node.exported_linker_flags:
                            exported_linker_flags.extend(link_group_node.exported_linker_flags[0])
                            exported_linker_post_flags.extend(link_group_node.exported_linker_flags[1])

                links.append(link_info)

            soname = group
            if not is_actually_merged:
                soname = linkable_nodes[group_data.constituents[0]].default_soname
                debug_info.with_default_soname.append((soname, group_data.constituents[0]))

            debug_info.group_debug.setdefault(
                group,
                struct(
                    soname = soname,
                    merged = is_actually_merged,
                    constituents = real_constituents,
                    shlib_deps = shared_lib_deps,
                    exported_public_nodes = exported_public_nodes,
                    exported_linker_flags = exported_linker_flags,
                    exported_linker_post_flags = exported_linker_post_flags,
                ),
            )

            output_path = platform_output_path(soname)
            link_args = [LinkArgs(infos = links)]

            shared_lib = create_shared_lib(
                ctx,
                output_path = output_path,
                soname = soname,
                link_args = link_args,
                cxx_toolchain = cxx_toolchain,
                shared_lib_deps = shared_lib_deps,
                label = representative_label,
                can_be_asset = can_be_asset,
            )

            link_group_linkable_nodes[group] = LinkGroupLinkableNode(
                link = LinkInfo(
                    name = soname,
                    pre_flags = exported_linker_flags,
                    linkables = [SharedLibLinkable(
                        lib = shared_lib.lib.output,
                    )],
                    post_flags = exported_linker_post_flags,
                ),
                deps = link_group_deps,
                exported_deps = [],
                shared_lib = shared_lib,
                # exported linker flags for shared libs are in their linkinfo itself and are not exported from dependents
                exported_linker_flags = None,
            )
            group_shared_libs[soname] = MergedSharedLibrary(
                soname = soname,
                lib = shared_lib,
                apk_module = group_data.apk_module,
                solib_constituents = solib_constituents,
                is_actually_merged = is_actually_merged,
            )

        shared_libs_by_platform[platform] = group_shared_libs
        debug_info.missing_default_solibs.extend([d for d in merge_data.default_shared_libs if d not in included_default_solibs])

    return MergedLinkables(
        shared_libs_by_platform = shared_libs_by_platform,
        debug_info = debug_info_by_platform,
    )

# When linking shared libraries, by default, all symbols are exported from the library. In a
# particular application, though, many of those symbols may never be used. Ideally, in each apk,
# each shared library would only export the minimal set of symbols that are used by other libraries
# in the apk. This would allow the linker to remove any dead code within the library (the linker
# can strip all code that is unreachable from the set of exported symbols).
#
# The native relinker tries to remedy the situation. When enabled for an apk, the native
# relinker will take the set of libraries in the apk and relink them in reverse order telling the
# linker to only export those symbols that are referenced by a higher library.
#
# The way this works is that the relinker does a topological traversal of the linked libraries (i.e.
# top-down, visiting nodes before their dependencies, this is the opposite order of most things we do
# in a build) and does:
# 1. extract the set of global symbols by the original lib
# 2. intersect that with the set of undefined symbols in all transitive dependents (i.e. higher in the graph) and
#    rules for required symbols (Java_*, Jni_Onload, relinker blocklist)
# 3. write a version script that says to make public only those symbols from (2)
# 4. link the lib with the exact same link line + the version script. Note that this means that the relinked libraries each are
#    actually linked against non-relinked ones. This does mean there's some risk of not detecting missing symbols (though mostly
#    only if they are caused by the relinker changes themselves).
# 5. extract the list of undefined symbols in the relinked libs (i.e. those symbols needed from dependencies and what had been
#    used in (1) above from higher nodes).
def relink_libraries(ctx: AnalysisContext, libraries_by_platform: dict[str, dict[str, SharedLibrary]]) -> dict[str, dict[str, SharedLibrary]]:
    relinked_libraries_by_platform = {}
    for platform, shared_libraries in libraries_by_platform.items():
        cxx_toolchain = ctx.attrs._cxx_toolchain[platform][CxxToolchainInfo]

        relinked_libraries = relinked_libraries_by_platform.setdefault(platform, {})
        unsupported_libs = {}
        shlib_graph = {}
        rev_shlib_graph = {}
        for soname, solib in shared_libraries.items():
            shlib_graph[soname] = []
            rev_shlib_graph.setdefault(soname, [])
            if solib.shlib_deps == None or solib.link_args == None:
                unsupported_libs[soname] = True
            else:
                for dep in solib.shlib_deps:
                    shlib_graph[soname].append(dep)
                    rev_shlib_graph.setdefault(dep, []).append(soname)
        needed_symbols_files = {}
        for soname in pre_order_traversal(shlib_graph):
            if soname in unsupported_libs:
                relinked_libraries[soname] = shared_libraries[soname]
                continue

            original_shared_library = shared_libraries[soname]
            output_path = "xdso-dce-relinker-libs/{}/{}".format(platform, soname)

            provided_symbols_file = extract_provided_symbols(ctx, cxx_toolchain, original_shared_library.lib.output)
            needed_symbols_for_this = [needed_symbols_files.get(rdep) for rdep in rev_shlib_graph[soname]]
            relinker_version_script = ctx.actions.declare_output(output_path + ".relinker.version_script")
            create_relinker_version_script(
                ctx.actions,
                output = relinker_version_script,
                relinker_blocklist = [regex(s) for s in ctx.attrs.relinker_whitelist],
                provided_symbols = provided_symbols_file,
                needed_symbols = needed_symbols_for_this,
            )
            relinker_link_args = original_shared_library.link_args + [LinkArgs(flags = [cmd_args(relinker_version_script, format = "-Wl,--version-script={}")])]

            shared_lib = create_shared_lib(
                ctx,
                output_path = output_path,
                soname = soname,
                link_args = relinker_link_args,
                cxx_toolchain = cxx_toolchain,
                shared_lib_deps = original_shared_library.shlib_deps,
                label = original_shared_library.label,
                can_be_asset = original_shared_library.can_be_asset,
            )
            needed_symbols_from_this = extract_undefined_symbols(ctx, cxx_toolchain, shared_lib.lib.output)
            unioned_needed_symbols_file = ctx.actions.declare_output(output_path + ".all_needed_symbols")
            union_needed_symbols(ctx.actions, unioned_needed_symbols_file, needed_symbols_for_this + [needed_symbols_from_this])
            needed_symbols_files[soname] = unioned_needed_symbols_file

            relinked_libraries[soname] = shared_lib

    return relinked_libraries_by_platform

def extract_provided_symbols(ctx: AnalysisContext, toolchain: CxxToolchainInfo, lib: Artifact) -> Artifact:
    return extract_global_syms(ctx, toolchain, lib, "relinker_extract_provided_symbols")

def create_relinker_version_script(actions: AnalysisActions, relinker_blocklist: list[regex], output: Artifact, provided_symbols: Artifact, needed_symbols: list[Artifact]):
    def create_version_script(ctx, artifacts, outputs):
        all_needed_symbols = {}
        for symbols_file in needed_symbols:
            for line in artifacts[symbols_file].read_string().strip().split("\n"):
                all_needed_symbols[line] = True

        symbols_to_keep = []
        for symbol in artifacts[provided_symbols].read_string().strip().split("\n"):
            keep_symbol = False
            if symbol in all_needed_symbols:
                keep_symbol = True
            elif "JNI_OnLoad" in symbol:
                keep_symbol = True
            elif "Java_" in symbol:
                keep_symbol = True
            else:
                for pattern in relinker_blocklist:
                    if pattern.match(symbol):
                        keep_symbol = True
                        break

            if keep_symbol:
                symbols_to_keep.append(symbol)

        version_script = "{\n"
        if symbols_to_keep:
            version_script += "global:\n"
        for symbol in symbols_to_keep:
            version_script += "  {};\n".format(symbol)
        version_script += "local: *;\n"
        version_script += "};\n"
        ctx.actions.write(outputs[output], version_script)

    actions.dynamic_output(dynamic = needed_symbols + [provided_symbols], inputs = [], outputs = [output], f = create_version_script)

def extract_undefined_symbols(ctx: AnalysisContext, toolchain: CxxToolchainInfo, lib: Artifact) -> Artifact:
    return extract_undefined_syms(ctx, toolchain, lib, "relinker_extract_undefined_symbols")

def union_needed_symbols(actions: AnalysisActions, output: Artifact, needed_symbols: list[Artifact]):
    def compute_union(ctx, artifacts, outputs):
        unioned_symbols = {}
        for symbols_file in needed_symbols:
            for line in artifacts[symbols_file].read_string().strip().split("\n"):
                unioned_symbols[line] = True
        symbols = sorted(unioned_symbols.keys())
        ctx.actions.write(outputs[output], symbols)

    actions.dynamic_output(dynamic = needed_symbols, inputs = [], outputs = [output], f = compute_union)

def strip_lib(ctx: AnalysisContext, cxx_toolchain: CxxToolchainInfo, shlib: Artifact, output_path: [str, None] = None):
    strip_flags = cmd_args(get_strip_non_global_flags(cxx_toolchain))
    return strip_object(
        ctx,
        cxx_toolchain,
        shlib,
        strip_flags,
        output_path = output_path,
    )

def create_shared_lib(
        ctx: AnalysisContext,
        *,
        output_path: str,
        soname: str,
        link_args: list[LinkArgs],
        cxx_toolchain: CxxToolchainInfo,
        shared_lib_deps: list[str],
        label: Label,
        can_be_asset: bool) -> SharedLibrary:
    link_result = cxx_link_shared_library(
        ctx = ctx,
        output = output_path,
        name = soname,
        opts = link_options(
            links = link_args,
            link_execution_preference = LinkExecutionPreference("any"),
            identifier = output_path,
            strip = False,
            cxx_toolchain = cxx_toolchain,
        ),
    )

    shlib = link_result.linked_object
    return SharedLibrary(
        lib = shlib,
        stripped_lib = strip_lib(ctx, cxx_toolchain, shlib.output),
        shlib_deps = shared_lib_deps,
        link_args = link_args,
        can_be_asset = can_be_asset,
        for_primary_apk = False,
        soname = soname,
        label = label,
    )
