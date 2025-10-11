# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//android:android_providers.bzl", "merge_android_packageable_info")
load(
    "@prelude//java:java_library.bzl",
    "build_java_library",
    "split_on_archives_and_plain_files",
)
load(
    "@prelude//java:java_providers.bzl",
    "JavaLibraryInfo",
    "JavaPackagingDepTSet",
    "JavaPackagingInfo",
    "JavaProviders",
    "create_java_library_providers",
    "create_native_providers",
    "derive_compiling_deps",
    "to_list",
)
load(
    "@prelude//java:java_toolchain.bzl",
    "AbiGenerationMode",
    "JavaToolchainInfo",
)
load("@prelude//java/plugins:java_annotation_processor.bzl", "AnnotationProcessorProperties", "create_annotation_processor_properties", "create_ksp_annotation_processor_properties")
load("@prelude//java/plugins:java_plugin.bzl", "create_plugin_params")
load("@prelude//java/utils:java_more_utils.bzl", "get_path_separator_for_exec_os")
load("@prelude//java/utils:java_utils.bzl", "derive_javac", "get_abi_generation_mode", "get_class_to_source_map_info", "get_default_info", "get_java_version_attributes")
load("@prelude//jvm:nullsafe.bzl", "get_nullsafe_info")
load(
    "@prelude//kotlin:kotlin_toolchain.bzl",
    "KotlinToolchainInfo",
)
load("@prelude//kotlin:kotlin_utils.bzl", "get_kotlinc_compatible_target")
load("@prelude//kotlin:kotlincd_jar_creator.bzl", "create_jar_artifact_kotlincd")
load("@prelude//utils:utils.bzl", "is_any", "map_idx")

_JAVA_OR_KOTLIN_FILE_EXTENSION = [".java", ".kt"]

def _create_kotlin_sources(
        ctx: AnalysisContext,
        srcs: list[Artifact],
        deps: list[Dependency],
        annotation_processor_properties: AnnotationProcessorProperties,
        ksp_annotation_processor_properties: AnnotationProcessorProperties,
        additional_classpath_entries: list[Artifact]) -> (Artifact, [Artifact, None], [Artifact, None]):
    """
    Runs kotlinc on the provided kotlin sources.
    """

    kotlin_toolchain = ctx.attrs._kotlin_toolchain[KotlinToolchainInfo]
    compile_kotlin_tool = kotlin_toolchain.compile_kotlin[RunInfo]
    kotlinc = kotlin_toolchain.kotlinc[RunInfo]
    kotlinc_output = ctx.actions.declare_output("kotlinc_classes_output", dir = True)

    compile_kotlin_cmd = cmd_args([
        compile_kotlin_tool,
        "--kotlinc_output",
        kotlinc_output.as_output(),
    ])
    java_toolchain = ctx.attrs._java_toolchain[JavaToolchainInfo]
    zip_scrubber_args = ["--zip_scrubber", cmd_args(java_toolchain.zip_scrubber, delimiter = " ")]
    compile_kotlin_cmd.add(zip_scrubber_args)

    kotlinc_cmd_args = cmd_args([kotlinc])

    compiling_classpath = [] + additional_classpath_entries
    compiling_deps_tset = derive_compiling_deps(ctx.actions, None, deps + kotlin_toolchain.kotlinc_classpath)
    if compiling_deps_tset:
        compiling_classpath.extend(
            [compiling_dep.abi for compiling_dep in list(compiling_deps_tset.traverse())],
        )

    classpath_args = cmd_args(
        compiling_classpath,
        delimiter = get_path_separator_for_exec_os(ctx),
    )

    # write joined classpath string into args file
    classpath_args_file, _ = ctx.actions.write(
        "kotlinc_classpath",
        classpath_args,
        allow_args = True,
    )

    compile_kotlin_cmd.hidden([compiling_classpath])

    kotlinc_cmd_args.add(["-classpath"])
    kotlinc_cmd_args.add(cmd_args(classpath_args_file, format = "@{}"))

    module_name = ctx.label.package.replace("/", ".") + "." + ctx.label.name
    kotlinc_cmd_args.add(
        [
            "-module-name",
            module_name,
            "-no-stdlib",
            "-no-reflect",
        ] + ctx.attrs.extra_kotlinc_arguments,
    )

    jvm_target = get_kotlinc_compatible_target(ctx.attrs.target) if ctx.attrs.target else None
    if jvm_target:
        kotlinc_cmd_args.add([
            "-jvm-target",
            jvm_target,
        ])

    kapt_generated_sources_output = None
    if annotation_processor_properties.annotation_processors:
        compile_kotlin_cmd.add(["--kapt_annotation_processing_jar", kotlin_toolchain.annotation_processing_jar[JavaLibraryInfo].library_output.full_library])
        compile_kotlin_cmd.add(["--kapt_annotation_processors", ",".join([p for ap in annotation_processor_properties.annotation_processors for p in ap.processors])])
        compile_kotlin_cmd.add(["--kapt_annotation_processor_params", ";".join(annotation_processor_properties.annotation_processor_params)])

        annotation_processor_classpath_tsets = (
            filter(None, ([ap.deps for ap in annotation_processor_properties.annotation_processors])) +
            [dep[JavaPackagingInfo].packaging_deps for dep in [kotlin_toolchain.annotation_processing_jar, kotlin_toolchain.kotlin_stdlib]]
        )
        annotation_processor_classpath = ctx.actions.tset(
            JavaPackagingDepTSet,
            children = annotation_processor_classpath_tsets,
        ).project_as_args("full_jar_args")
        kapt_classpath_file = ctx.actions.write("kapt_classpath_file", annotation_processor_classpath)
        compile_kotlin_cmd.add(["--kapt_classpath_file", kapt_classpath_file])
        compile_kotlin_cmd.hidden(annotation_processor_classpath)

        sources_output = ctx.actions.declare_output("kapt_sources_output")
        compile_kotlin_cmd.add(["--kapt_sources_output", sources_output.as_output()])
        classes_output = ctx.actions.declare_output("kapt_classes_output")
        compile_kotlin_cmd.add(["--kapt_classes_output", classes_output.as_output()])
        stubs = ctx.actions.declare_output("kapt_stubs")
        compile_kotlin_cmd.add(["--kapt_stubs", stubs.as_output()])

        kapt_generated_sources_output = ctx.actions.declare_output("kapt_generated_sources_output.src.zip")
        compile_kotlin_cmd.add(["--kapt_generated_sources_output", kapt_generated_sources_output.as_output()])
        compile_kotlin_cmd.add(["--kapt_base64_encoder", cmd_args(kotlin_toolchain.kapt_base64_encoder[RunInfo], delimiter = " ")])
        generated_kotlin_output = ctx.actions.declare_output("kapt_generated_kotlin_output")
        compile_kotlin_cmd.add(["--kapt_generated_kotlin_output", generated_kotlin_output.as_output()])
        if jvm_target:
            compile_kotlin_cmd.add(["--kapt_jvm_target", jvm_target])

    friend_paths = ctx.attrs.friend_paths
    if friend_paths:
        concat_friends_paths = cmd_args([friend_path.library_output.abi for friend_path in map_idx(JavaLibraryInfo, friend_paths) if friend_path.library_output], delimiter = ",")
        kotlinc_cmd_args.add(cmd_args(["-Xfriend-paths", concat_friends_paths], delimiter = "="))

    zipped_sources, plain_sources = split_on_archives_and_plain_files(srcs, _JAVA_OR_KOTLIN_FILE_EXTENSION)

    kotlinc_cmd_args.add(plain_sources)

    ksp_zipped_sources_output = None
    if ksp_annotation_processor_properties.annotation_processors:
        ksp_cmd = cmd_args(compile_kotlin_tool)
        ksp_cmd.add(zip_scrubber_args)

        ksp_annotation_processor_classpath_tsets = filter(None, ([ap.deps for ap in ksp_annotation_processor_properties.annotation_processors]))
        if ksp_annotation_processor_classpath_tsets:
            ksp_annotation_processor_classpath = ctx.actions.tset(
                JavaPackagingDepTSet,
                children = ksp_annotation_processor_classpath_tsets,
            ).project_as_args("full_jar_args")
            ksp_cmd.add(["--ksp_processor_jars"])
            ksp_cmd.add(cmd_args(ksp_annotation_processor_classpath, delimiter = ","))

        ksp_cmd.add(["--ksp_classpath", classpath_args])
        ksp_classes_and_resources_output = ctx.actions.declare_output("ksp_output_dir/ksp_classes_and_resources_output")
        ksp_cmd.add(["--ksp_classes_and_resources_output", ksp_classes_and_resources_output.as_output()])
        ksp_output = cmd_args(ksp_classes_and_resources_output.as_output()).parent()
        ksp_cmd.add(["--ksp_output", ksp_output])
        ksp_sources_output = ctx.actions.declare_output("ksp_output_dir/ksp_sources_output")
        ksp_cmd.add(["--ksp_sources_output", ksp_sources_output.as_output()])
        ksp_zipped_sources_output = ctx.actions.declare_output("ksp_output_dir/ksp_zipped_sources_output.src.zip")
        ksp_cmd.add(["--ksp_zipped_sources_output", ksp_zipped_sources_output.as_output()])
        ksp_cmd.add(["--ksp_project_base_dir", ctx.label.path])

        ksp_kotlinc_cmd_args = cmd_args(kotlinc_cmd_args)
        _add_plugins(ctx, ksp_kotlinc_cmd_args, ksp_cmd, is_ksp = True)

        ksp_cmd_args_file, _ = ctx.actions.write(
            "ksp_kotlinc_cmd",
            ksp_kotlinc_cmd_args,
            allow_args = True,
        )

        ksp_cmd.add("--kotlinc_cmd_file")
        ksp_cmd.add(ksp_cmd_args_file)
        ksp_cmd.hidden(ksp_kotlinc_cmd_args)

        ctx.actions.run(ksp_cmd, category = "ksp_kotlinc")

        zipped_sources = (zipped_sources or []) + [ksp_zipped_sources_output]
        compile_kotlin_cmd.add(["--ksp_generated_classes_and_resources", ksp_classes_and_resources_output])

    _add_plugins(ctx, kotlinc_cmd_args, compile_kotlin_cmd, is_ksp = False)

    if zipped_sources:
        zipped_sources_file = ctx.actions.write("kotlinc_zipped_source_args", zipped_sources)
        compile_kotlin_cmd.add(["--zipped_sources_file", zipped_sources_file])
        compile_kotlin_cmd.hidden(zipped_sources)

    args_file, _ = ctx.actions.write(
        "kotlinc_cmd",
        kotlinc_cmd_args,
        allow_args = True,
    )

    compile_kotlin_cmd.hidden([plain_sources])

    compile_kotlin_cmd.add("--kotlinc_cmd_file")
    compile_kotlin_cmd.add(args_file)
    compile_kotlin_cmd.hidden(kotlinc_cmd_args)

    ctx.actions.run(compile_kotlin_cmd, category = "kotlinc")

    return kotlinc_output, kapt_generated_sources_output, ksp_zipped_sources_output

def _is_ksp_plugin(plugin: str) -> bool:
    return "symbol-processing" in plugin

def _add_plugins(
        ctx: AnalysisContext,
        kotlinc_cmd_args: cmd_args,
        compile_kotlin_cmd: cmd_args,
        is_ksp: bool):
    for plugin, plugin_options in ctx.attrs.kotlin_compiler_plugins.items():
        if _is_ksp_plugin(str(plugin)) != is_ksp:
            continue

        kotlinc_cmd_args.add(cmd_args(["-Xplugin", plugin], delimiter = "="))
        options = []
        for option_key, option_val in plugin_options.items():
            # "_codegen_dir_" means buck should provide a dir
            if option_val == "__codegen_dir__":
                option_val = ctx.actions.declare_output("kotlin_compiler_plugin_dir")
                options.append(cmd_args([option_key, option_val.as_output()], delimiter = "="))
                compile_kotlin_cmd.add(["--kotlin_compiler_plugin_dir", option_val.as_output()])
            else:
                options.append(cmd_args([option_key, option_val], delimiter = "="))

        if options:
            kotlinc_cmd_args.add(["-P", cmd_args(options, delimiter = ",")])

def kotlin_library_impl(ctx: AnalysisContext) -> list[Provider]:
    packaging_deps = ctx.attrs.deps + ctx.attrs.exported_deps + ctx.attrs.runtime_deps

    # TODO(T107163344) this shouldn't be in kotlin_library itself, use overlays to remove it.
    android_packageable_info = merge_android_packageable_info(ctx.label, ctx.actions, packaging_deps)
    if ctx.attrs._build_only_native_code:
        shared_library_info, cxx_resource_info, linkable_graph = create_native_providers(ctx, ctx.label, packaging_deps)
        return [
            shared_library_info,
            cxx_resource_info,
            linkable_graph,
            # Add an unused default output in case this target is used an an attr.source() anywhere.
            DefaultInfo(default_output = ctx.actions.write("{}/unused.jar".format(ctx.label.name), [])),
            TemplatePlaceholderInfo(keyed_variables = {
                "classpath": "unused_but_needed_for_analysis",
            }),
            android_packageable_info,
        ]

    java_providers = build_kotlin_library(ctx)
    return to_list(java_providers) + [android_packageable_info]

def build_kotlin_library(
        ctx: AnalysisContext,
        additional_classpath_entries: list[Artifact] = [],
        bootclasspath_entries: list[Artifact] = [],
        extra_sub_targets: dict = {}) -> JavaProviders:
    srcs = ctx.attrs.srcs
    has_kotlin_srcs = is_any(lambda src: src.extension == ".kt" or src.basename.endswith(".src.zip") or src.basename.endswith("-sources.jar"), srcs)

    if not has_kotlin_srcs:
        return build_java_library(
            ctx,
            ctx.attrs.srcs,
            bootclasspath_entries = bootclasspath_entries,
            additional_classpath_entries = additional_classpath_entries,
            # Match buck1, which always does class ABI generation for Kotlin targets unless explicitly specified.
            override_abi_generation_mode = get_abi_generation_mode(ctx.attrs.abi_generation_mode) or AbiGenerationMode("class"),
            extra_sub_targets = extra_sub_targets,
        )

    else:
        deps_query = getattr(ctx.attrs, "deps_query", []) or []
        provided_deps_query = getattr(ctx.attrs, "provided_deps_query", []) or []
        deps = (
            ctx.attrs.deps +
            deps_query +
            ctx.attrs.exported_deps +
            ctx.attrs.provided_deps +
            provided_deps_query +
            ctx.attrs.exported_provided_deps
        )
        annotation_processor_properties = create_annotation_processor_properties(
            ctx,
            ctx.attrs.plugins,
            ctx.attrs.annotation_processors,
            ctx.attrs.annotation_processor_params,
            ctx.attrs.annotation_processor_deps,
        )
        ksp_annotation_processor_properties = create_ksp_annotation_processor_properties(ctx, ctx.attrs.plugins)

        kotlin_toolchain = ctx.attrs._kotlin_toolchain[KotlinToolchainInfo]
        if kotlin_toolchain.kotlinc_protocol == "classic":
            kotlinc_classes, kapt_generated_sources, ksp_generated_sources = _create_kotlin_sources(
                ctx,
                ctx.attrs.srcs,
                deps,
                annotation_processor_properties,
                ksp_annotation_processor_properties,
                # kotlic doesn't support -bootclasspath param, so adding `bootclasspath_entries` into kotlin classpath
                additional_classpath_entries + bootclasspath_entries,
            )
            srcs = [src for src in ctx.attrs.srcs if not src.extension == ".kt"]
            if kapt_generated_sources:
                srcs.append(kapt_generated_sources)
            if ksp_generated_sources:
                srcs.append(ksp_generated_sources)
            java_lib = build_java_library(
                ctx,
                srcs,
                run_annotation_processors = False,
                bootclasspath_entries = bootclasspath_entries,
                additional_classpath_entries = [kotlinc_classes] + additional_classpath_entries,
                additional_compiled_srcs = kotlinc_classes,
                generated_sources = filter(None, [kapt_generated_sources, ksp_generated_sources]),
                extra_sub_targets = extra_sub_targets,
            )
            return java_lib
        elif kotlin_toolchain.kotlinc_protocol == "kotlincd":
            source_level, target_level = get_java_version_attributes(ctx)
            extra_arguments = cmd_args(ctx.attrs.extra_arguments)
            common_kotlincd_kwargs = {
                "abi_generation_mode": get_abi_generation_mode(ctx.attrs.abi_generation_mode),
                "actions": ctx.actions,
                "additional_classpath_entries": additional_classpath_entries,
                "annotation_processor_properties": AnnotationProcessorProperties(
                    annotation_processors = annotation_processor_properties.annotation_processors + ksp_annotation_processor_properties.annotation_processors,
                    annotation_processor_params = annotation_processor_properties.annotation_processor_params + ksp_annotation_processor_properties.annotation_processor_params,
                ),
                "bootclasspath_entries": bootclasspath_entries,
                "deps": deps,
                "extra_kotlinc_arguments": ctx.attrs.extra_kotlinc_arguments,
                "friend_paths": ctx.attrs.friend_paths,
                "is_building_android_binary": ctx.attrs._is_building_android_binary,
                "java_toolchain": ctx.attrs._java_toolchain[JavaToolchainInfo],
                "javac_tool": derive_javac(ctx.attrs.javac) if ctx.attrs.javac else None,
                "k2": ctx.attrs.k2,
                "kotlin_compiler_plugins": ctx.attrs.kotlin_compiler_plugins,
                "kotlin_toolchain": kotlin_toolchain,
                "label": ctx.label,
                "remove_classes": ctx.attrs.remove_classes,
                "required_for_source_only_abi": ctx.attrs.required_for_source_only_abi,
                "resources": ctx.attrs.resources,
                "resources_root": ctx.attrs.resources_root,
                "source_level": source_level,
                "source_only_abi_deps": ctx.attrs.source_only_abi_deps,
                "srcs": srcs,
                "target_level": target_level,
            }
            outputs = create_jar_artifact_kotlincd(
                plugin_params = create_plugin_params(ctx, ctx.attrs.plugins),
                extra_arguments = extra_arguments,
                actions_identifier = "",
                **common_kotlincd_kwargs
            )

            if outputs and outputs.annotation_processor_output:
                generated_sources = [outputs.annotation_processor_output]
                extra_sub_targets = extra_sub_targets | {"generated_sources": [
                    DefaultInfo(default_output = outputs.annotation_processor_output),
                ]}
            else:
                generated_sources = []

            java_toolchain = ctx.attrs._java_toolchain[JavaToolchainInfo]
            if (
                not java_toolchain.is_bootstrap_toolchain and
                not ctx.attrs._is_building_android_binary
            ):
                nullsafe_info = get_nullsafe_info(ctx)
                if nullsafe_info:
                    create_jar_artifact_kotlincd(
                        actions_identifier = "nullsafe",
                        plugin_params = nullsafe_info.plugin_params,
                        extra_arguments = nullsafe_info.extra_arguments,
                        # To make sure that even for pure Kotlin targets empty output dir is always present
                        optional_dirs = [nullsafe_info.output.as_output()],
                        is_creating_subtarget = True,
                        **common_kotlincd_kwargs
                    )

                    extra_sub_targets = extra_sub_targets | {"nullsafex-json": [
                        DefaultInfo(default_output = nullsafe_info.output),
                    ]}

            java_library_info, java_packaging_info, shared_library_info, cxx_resource_info, linkable_graph, template_placeholder_info, intellij_info = create_java_library_providers(
                ctx,
                library_output = outputs.classpath_entry if outputs else None,
                declared_deps = ctx.attrs.deps + deps_query,
                exported_deps = ctx.attrs.exported_deps,
                provided_deps = ctx.attrs.provided_deps + provided_deps_query,
                exported_provided_deps = ctx.attrs.exported_provided_deps,
                runtime_deps = ctx.attrs.runtime_deps,
                needs_desugar = source_level > 7 or target_level > 7,
                generated_sources = generated_sources,
                has_srcs = bool(srcs),
            )

            class_to_src_map, class_to_src_map_sub_targets = get_class_to_source_map_info(
                ctx,
                outputs = outputs,
                deps = ctx.attrs.deps + deps_query + ctx.attrs.exported_deps,
            )
            extra_sub_targets = extra_sub_targets | class_to_src_map_sub_targets

            default_info = get_default_info(
                ctx.actions,
                ctx.attrs._java_toolchain[JavaToolchainInfo],
                outputs,
                java_packaging_info,
                extra_sub_targets = extra_sub_targets,
            )
            return JavaProviders(
                java_library_info = java_library_info,
                java_library_intellij_info = intellij_info,
                java_packaging_info = java_packaging_info,
                shared_library_info = shared_library_info,
                cxx_resource_info = cxx_resource_info,
                linkable_graph = linkable_graph,
                template_placeholder_info = template_placeholder_info,
                default_info = default_info,
                class_to_src_map = class_to_src_map,
            )
        else:
            fail("unrecognized kotlinc protocol `{}`".format(kotlin_toolchain.kotlinc_protocol))
