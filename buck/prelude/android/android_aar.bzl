# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//android:android_binary.bzl", "get_build_config_java_libraries")
load("@prelude//android:android_binary_native_library_rules.bzl", "get_android_binary_native_library_info")
load("@prelude//android:android_binary_resources_rules.bzl", "get_cxx_resources", "get_manifest")
load("@prelude//android:android_providers.bzl", "AndroidResourceInfo", "ExportedAndroidResourceInfo", "merge_android_packageable_info")
load("@prelude//android:android_resource.bzl", "get_text_symbols")
load("@prelude//android:android_toolchain.bzl", "AndroidToolchainInfo")
load("@prelude//android:configuration.bzl", "get_deps_by_platform")
load("@prelude//android:cpu_filters.bzl", "CPU_FILTER_FOR_DEFAULT_PLATFORM", "CPU_FILTER_FOR_PRIMARY_PLATFORM")
load("@prelude//android:util.bzl", "create_enhancement_context")
load("@prelude//java:java_providers.bzl", "get_all_java_packaging_deps", "get_all_java_packaging_deps_from_packaging_infos")
load("@prelude//java:java_toolchain.bzl", "JavaToolchainInfo")

def android_aar_impl(ctx: AnalysisContext) -> list[Provider]:
    deps_by_platform = get_deps_by_platform(ctx)
    primary_platform = CPU_FILTER_FOR_PRIMARY_PLATFORM if CPU_FILTER_FOR_PRIMARY_PLATFORM in deps_by_platform else CPU_FILTER_FOR_DEFAULT_PLATFORM
    deps = deps_by_platform[primary_platform]

    java_packaging_deps = [packaging_dep for packaging_dep in get_all_java_packaging_deps(ctx, deps)]
    android_packageable_info = merge_android_packageable_info(ctx.label, ctx.actions, deps)

    android_manifest = get_manifest(ctx, android_packageable_info, manifest_entries = {})

    if ctx.attrs.include_build_config_class:
        build_config_infos = list(android_packageable_info.build_config_infos.traverse()) if android_packageable_info.build_config_infos else []
        java_packaging_deps.extend(get_all_java_packaging_deps_from_packaging_infos(
            ctx,
            get_build_config_java_libraries(ctx, build_config_infos, package_type = "release", exopackage_modes = []),
        ))

    jars = [dep.jar for dep in java_packaging_deps if dep.jar]
    classes_jar = ctx.actions.declare_output("classes.jar")
    java_toolchain = ctx.attrs._java_toolchain[JavaToolchainInfo]
    classes_jar_cmd = cmd_args([
        java_toolchain.jar_builder,
        "--entries-to-jar",
        ctx.actions.write("classes_jar_entries.txt", jars),
        "--output",
        classes_jar.as_output(),
    ]).hidden(jars)

    if ctx.attrs.remove_classes:
        remove_classes_file = ctx.actions.write("remove_classes.txt", ctx.attrs.remove_classes)
        classes_jar_cmd.add([
            "--blocklist-patterns",
            remove_classes_file,
            "--blocklist-patterns-matcher",
            "remove_classes_patterns_matcher",
        ])

    ctx.actions.run(classes_jar_cmd, category = "create_classes_jar")

    entries = [android_manifest, classes_jar]

    resource_infos = list(android_packageable_info.resource_infos.traverse()) if android_packageable_info.resource_infos else []

    android_toolchain = ctx.attrs._android_toolchain[AndroidToolchainInfo]
    if resource_infos:
        res_dirs = [resource_info.res for resource_info in resource_infos if resource_info.res]
        merged_resource_sources_dir = ctx.actions.declare_output("merged_resource_sources_dir/res", dir = True)
        merge_resource_sources_cmd = cmd_args([
            android_toolchain.merge_android_resource_sources[RunInfo],
            "--resource-paths",
            ctx.actions.write("resource_paths.txt", res_dirs),
            "--output",
            merged_resource_sources_dir.as_output(),
        ]).hidden(res_dirs)

        ctx.actions.run(merge_resource_sources_cmd, category = "merge_android_resource_sources")

        r_dot_txt = get_text_symbols(ctx, merged_resource_sources_dir, [dep for dep in deps if AndroidResourceInfo in dep or ExportedAndroidResourceInfo in dep])
        entries.extend([merged_resource_sources_dir, r_dot_txt])

        assets_dirs = [resource_infos.assets for resource_infos in resource_infos if resource_infos.assets]
        entries.extend(assets_dirs)

    cxx_resources = get_cxx_resources(ctx, deps, dir_name = "assets")
    if cxx_resources:
        entries.append(cxx_resources)

    enhancement_ctx = create_enhancement_context(ctx)
    android_binary_native_library_info = get_android_binary_native_library_info(enhancement_ctx, android_packageable_info, deps_by_platform)
    native_libs_file = ctx.actions.write("native_libs_entries.txt", android_binary_native_library_info.native_libs_for_primary_apk)
    native_libs_assets_file = ctx.actions.write("native_libs_assets_entries.txt", android_binary_native_library_info.root_module_native_lib_assets)

    entries_file = ctx.actions.write("entries.txt", entries)

    aar = ctx.actions.declare_output("{}.aar".format(ctx.label.name))
    create_aar_cmd = cmd_args([
        android_toolchain.aar_builder,
        "--output_path",
        aar.as_output(),
        "--entries_file",
        entries_file,
        "--on_duplicate_entry",
        "fail",
        "--native_libs_file",
        native_libs_file,
        "--native_libs_assets_file",
        native_libs_assets_file,
    ]).hidden(entries, android_binary_native_library_info.native_libs_for_primary_apk, android_binary_native_library_info.root_module_native_lib_assets)

    ctx.actions.run(create_aar_cmd, category = "create_aar")

    return [DefaultInfo(default_outputs = [aar], sub_targets = enhancement_ctx.get_sub_targets())]
