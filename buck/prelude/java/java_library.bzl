# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//:paths.bzl", "paths")
load("@prelude//android:android_providers.bzl", "merge_android_packageable_info")
load(
    "@prelude//java:java_providers.bzl",
    "JavaCompileOutputs",  # @unused Used as type
    "JavaLibraryInfo",
    "JavaPackagingDepTSet",
    "JavaProviders",
    "create_abi",
    "create_java_library_providers",
    "create_native_providers",
    "derive_compiling_deps",
    "make_compile_outputs",
    "to_list",
)
load("@prelude//java:java_resources.bzl", "get_resources_map")
load("@prelude//java:java_toolchain.bzl", "AbiGenerationMode", "JavaToolchainInfo")
load("@prelude//java:javacd_jar_creator.bzl", "create_jar_artifact_javacd")
load("@prelude//java/plugins:java_annotation_processor.bzl", "AnnotationProcessorProperties", "create_annotation_processor_properties")
load(
    "@prelude//java/plugins:java_plugin.bzl",
    "PluginParams",  # @unused Used as type
    "create_plugin_params",
)
load("@prelude//java/utils:java_more_utils.bzl", "get_path_separator_for_exec_os")
load("@prelude//java/utils:java_utils.bzl", "declare_prefixed_name", "derive_javac", "get_abi_generation_mode", "get_class_to_source_map_info", "get_default_info", "get_java_version_attributes", "to_java_version")
load("@prelude//jvm:nullsafe.bzl", "get_nullsafe_info")
load("@prelude//linking:shared_libraries.bzl", "SharedLibraryInfo")
load("@prelude//utils:utils.bzl", "expect")

_JAVA_FILE_EXTENSION = [".java"]
_SUPPORTED_ARCHIVE_SUFFIXES = [".src.zip", "-sources.jar"]

def _process_classpath(
        actions: AnalysisActions,
        classpath_args: cmd_args,
        cmd: cmd_args,
        args_file_name: str,
        option_name: str):
    # write joined classpath string into args file
    classpath_args_file, _ = actions.write(
        args_file_name,
        classpath_args,
        allow_args = True,
    )

    # mark classpath artifacts as input
    cmd.hidden(classpath_args)

    # add classpath args file to cmd
    cmd.add(option_name, classpath_args_file)

def classpath_args(ctx: AnalysisContext, args):
    return cmd_args(args, delimiter = get_path_separator_for_exec_os(ctx))

def _process_plugins(
        ctx: AnalysisContext,
        actions_identifier: [str, None],
        annotation_processor_properties: AnnotationProcessorProperties,
        plugin_params: [PluginParams, None],
        javac_args: cmd_args,
        cmd: cmd_args):
    processors_classpath_tsets = []

    # Process Annotation processors
    if annotation_processor_properties.annotation_processors:
        # For external javac, we can't preserve separate classpaths for separate processors. So we just concat everything.
        javac_args.add("-processor")
        joined_processors_string = ",".join([p for ap in annotation_processor_properties.annotation_processors for p in ap.processors])

        javac_args.add(joined_processors_string)
        for param in annotation_processor_properties.annotation_processor_params:
            javac_args.add("-A{}".format(param))

        for ap in annotation_processor_properties.annotation_processors:
            if ap.deps:
                processors_classpath_tsets.append(ap.deps)

    else:
        javac_args.add("-proc:none")

    # Process Javac Plugins
    if plugin_params:
        plugin = plugin_params.processors[0]
        args = plugin_params.args.get(plugin, cmd_args())

        # Produces "-Xplugin:PluginName arg1 arg2 arg3", as a single argument
        plugin_and_args = cmd_args(plugin)
        plugin_and_args.add(args)
        plugin_arg = cmd_args(format = "-Xplugin:{}", quote = "shell")
        plugin_arg.add(cmd_args(plugin_and_args, delimiter = " "))

        javac_args.add(plugin_arg)
        if plugin_params.deps:
            processors_classpath_tsets.append(plugin_params.deps)

    if len(processors_classpath_tsets) > 1:
        processors_classpath_tset = ctx.actions.tset(JavaPackagingDepTSet, children = processors_classpath_tsets)
    elif len(processors_classpath_tsets) == 1:
        processors_classpath_tset = processors_classpath_tsets[0]
    else:
        processors_classpath_tset = None

    if processors_classpath_tset:
        processors_classpath = classpath_args(ctx, processors_classpath_tset.project_as_args("full_jar_args"))
        _process_classpath(
            ctx.actions,
            processors_classpath,
            cmd,
            declare_prefixed_name("plugin_cp_args", actions_identifier),
            "--javac_processors_classpath_file",
        )

def _build_classpath(actions: AnalysisActions, deps: list[Dependency], additional_classpath_entries: list[Artifact], classpath_args_projection: str) -> [cmd_args, None]:
    compiling_deps_tset = derive_compiling_deps(actions, None, deps)

    if additional_classpath_entries or compiling_deps_tset:
        args = cmd_args()
        if compiling_deps_tset:
            args.add(compiling_deps_tset.project_as_args(classpath_args_projection))
        args.add(additional_classpath_entries)
        return args

    return None

def _build_bootclasspath(bootclasspath_entries: list[Artifact], source_level: int, java_toolchain: JavaToolchainInfo) -> list[Artifact]:
    bootclasspath_list = []
    if source_level in [7, 8]:
        if bootclasspath_entries:
            bootclasspath_list = bootclasspath_entries
        elif source_level == 7:
            bootclasspath_list = java_toolchain.bootclasspath_7
        elif source_level == 8:
            bootclasspath_list = java_toolchain.bootclasspath_8
    return bootclasspath_list

def _append_javac_params(
        ctx: AnalysisContext,
        actions_identifier: [str, None],
        java_toolchain: JavaToolchainInfo,
        srcs: list[Artifact],
        remove_classes: list[str],
        annotation_processor_properties: AnnotationProcessorProperties,
        javac_plugin_params: [PluginParams, None],
        source_level: int,
        target_level: int,
        deps: list[Dependency],
        extra_arguments: cmd_args,
        additional_classpath_entries: list[Artifact],
        bootclasspath_entries: list[Artifact],
        cmd: cmd_args,
        generated_sources_dir: Artifact):
    javac_args = cmd_args(
        "-encoding",
        "utf-8",
        # Set the sourcepath to stop us reading source files out of jars by mistake.
        "-sourcepath",
        '""',
    )
    javac_args.add(extra_arguments)

    compiling_classpath = _build_classpath(ctx.actions, deps, additional_classpath_entries, "args_for_compiling")
    if compiling_classpath:
        _process_classpath(
            ctx.actions,
            classpath_args(ctx, compiling_classpath),
            cmd,
            declare_prefixed_name("classpath_args", actions_identifier),
            "--javac_classpath_file",
        )
    else:
        javac_args.add("-classpath ''")

    javac_args.add("-source")
    javac_args.add(str(source_level))
    javac_args.add("-target")
    javac_args.add(str(target_level))

    bootclasspath_list = _build_bootclasspath(bootclasspath_entries, source_level, java_toolchain)
    if bootclasspath_list:
        _process_classpath(
            ctx.actions,
            classpath_args(ctx, bootclasspath_list),
            cmd,
            declare_prefixed_name("bootclasspath_args", actions_identifier),
            "--javac_bootclasspath_file",
        )

    _process_plugins(
        ctx,
        actions_identifier,
        annotation_processor_properties,
        javac_plugin_params,
        javac_args,
        cmd,
    )

    cmd.add("--generated_sources_dir", generated_sources_dir.as_output())

    zipped_sources, plain_sources = split_on_archives_and_plain_files(srcs, _JAVA_FILE_EXTENSION)

    javac_args.add(*plain_sources)
    args_file, _ = ctx.actions.write(
        declare_prefixed_name("javac_args", actions_identifier),
        javac_args,
        allow_args = True,
    )
    cmd.hidden(javac_args)

    # mark plain srcs artifacts as input
    cmd.hidden(plain_sources)

    cmd.add("--javac_args_file", args_file)

    if zipped_sources:
        cmd.add("--zipped_sources_file", ctx.actions.write(declare_prefixed_name("zipped_source_args", actions_identifier), zipped_sources))
        cmd.hidden(zipped_sources)

    if remove_classes:
        cmd.add("--remove_classes", ctx.actions.write(declare_prefixed_name("remove_classes_args", actions_identifier), remove_classes))

def split_on_archives_and_plain_files(
        srcs: list[Artifact],
        plain_file_extensions: list[str]) -> (list[Artifact], list[Artifact]):
    archives = []
    plain_sources = []

    for src in srcs:
        if src.extension in plain_file_extensions:
            plain_sources.append(src)
        elif _is_supported_archive(src):
            archives.append(src)
        else:
            fail("Provided java source is not supported: {}".format(src))

    return (archives, plain_sources)

def _is_supported_archive(src: Artifact) -> bool:
    basename = src.basename
    for supported_suffix in _SUPPORTED_ARCHIVE_SUFFIXES:
        if basename.endswith(supported_suffix):
            return True
    return False

def _copy_resources(
        actions: AnalysisActions,
        actions_identifier: [str, None],
        java_toolchain: JavaToolchainInfo,
        package: str,
        resources: list[Artifact],
        resources_root: [str, None]) -> Artifact:
    resources_to_copy = get_resources_map(java_toolchain, package, resources, resources_root)
    resource_output = actions.symlinked_dir(declare_prefixed_name("resources", actions_identifier), resources_to_copy)
    return resource_output

def _jar_creator(
        javac_tool: [typing.Any, None],
        java_toolchain: JavaToolchainInfo) -> typing.Callable:
    if javac_tool or java_toolchain.javac_protocol == "classic":
        return _create_jar_artifact
    elif java_toolchain.javac_protocol == "javacd":
        return create_jar_artifact_javacd
    else:
        fail("unrecognized javac protocol `{}`".format(java_toolchain.javac_protocol))

def compile_to_jar(
        ctx: AnalysisContext,
        srcs: list[Artifact],
        *,
        abi_generation_mode: [AbiGenerationMode, None] = None,
        output: [Artifact, None] = None,
        actions_identifier: [str, None] = None,
        javac_tool: [typing.Any, None] = None,
        resources: [list[Artifact], None] = None,
        resources_root: [str, None] = None,
        remove_classes: [list[str], None] = None,
        manifest_file: [Artifact, None] = None,
        annotation_processor_properties: [AnnotationProcessorProperties, None] = None,
        plugin_params: [PluginParams, None] = None,
        source_level: [int, None] = None,
        target_level: [int, None] = None,
        deps: [list[Dependency], None] = None,
        required_for_source_only_abi: bool = False,
        source_only_abi_deps: [list[Dependency], None] = None,
        extra_arguments: [cmd_args, None] = None,
        additional_classpath_entries: [list[Artifact], None] = None,
        additional_compiled_srcs: [Artifact, None] = None,
        bootclasspath_entries: [list[Artifact], None] = None,
        is_creating_subtarget: bool = False) -> JavaCompileOutputs:
    if not additional_classpath_entries:
        additional_classpath_entries = []
    if not bootclasspath_entries:
        bootclasspath_entries = []
    if not extra_arguments:
        extra_arguments = cmd_args()
    if not resources:
        resources = []
    if not deps:
        deps = []
    if not remove_classes:
        remove_classes = []
    if not annotation_processor_properties:
        annotation_processor_properties = AnnotationProcessorProperties(annotation_processors = [], annotation_processor_params = [])
    if not source_only_abi_deps:
        source_only_abi_deps = []

    # TODO(cjhopman): Should verify that source_only_abi_deps are contained within the normal classpath.

    java_toolchain = ctx.attrs._java_toolchain[JavaToolchainInfo]
    if not source_level:
        source_level = to_java_version(java_toolchain.source_level)
    if not target_level:
        target_level = to_java_version(java_toolchain.target_level)

    is_building_android_binary = ctx.attrs._is_building_android_binary

    return _jar_creator(javac_tool, java_toolchain)(
        ctx,
        actions_identifier,
        abi_generation_mode,
        java_toolchain,
        ctx.label,
        output,
        javac_tool,
        srcs,
        remove_classes,
        resources,
        resources_root,
        manifest_file,
        annotation_processor_properties,
        plugin_params,
        source_level,
        target_level,
        deps,
        required_for_source_only_abi,
        source_only_abi_deps,
        extra_arguments,
        additional_classpath_entries,
        additional_compiled_srcs,
        bootclasspath_entries,
        is_building_android_binary,
        is_creating_subtarget,
    )

def _create_jar_artifact(
        ctx: AnalysisContext,
        actions_identifier: [str, None],
        abi_generation_mode: [AbiGenerationMode, None],
        java_toolchain: JavaToolchainInfo,
        label: Label,
        output: [Artifact, None],
        javac_tool: [typing.Any, None],
        srcs: list[Artifact],
        remove_classes: list[str],
        resources: list[Artifact],
        resources_root: [str, None],
        manifest_file: [Artifact, None],
        annotation_processor_properties: AnnotationProcessorProperties,
        plugin_params: [PluginParams, None],
        source_level: int,
        target_level: int,
        deps: list[Dependency],
        required_for_source_only_abi: bool,
        _source_only_abi_deps: list[Dependency],
        extra_arguments: cmd_args,
        additional_classpath_entries: list[Artifact],
        additional_compiled_srcs: [Artifact, None],
        bootclasspath_entries: list[Artifact],
        _is_building_android_binary: bool,
        _is_creating_subtarget: bool = False) -> JavaCompileOutputs:
    """
    Creates jar artifact.

    Returns a single artifacts that represents jar output file
    """
    javac_tool = javac_tool or derive_javac(java_toolchain.javac)
    jar_out = output or ctx.actions.declare_output(paths.join(actions_identifier or "jar", "{}.jar".format(label.name)))

    args = [
        java_toolchain.compile_and_package[RunInfo],
        "--jar_builder_tool",
        cmd_args(java_toolchain.jar_builder, delimiter = " "),
        "--output",
        jar_out.as_output(),
    ]

    skip_javac = False if srcs or annotation_processor_properties.annotation_processors or plugin_params else True
    if skip_javac:
        args.append("--skip_javac_run")
    else:
        args += ["--javac_tool", javac_tool]

    if resources:
        resource_dir = _copy_resources(ctx.actions, actions_identifier, java_toolchain, label.package, resources, resources_root)
        args += ["--resources_dir", resource_dir]

    if manifest_file:
        args += ["--manifest", manifest_file]

    if additional_compiled_srcs:
        args += ["--additional_compiled_srcs", additional_compiled_srcs]

    compile_and_package_cmd = cmd_args(args)

    generated_sources_dir = None
    if not skip_javac:
        generated_sources_dir = ctx.actions.declare_output(declare_prefixed_name("generated_sources", actions_identifier), dir = True)
        _append_javac_params(
            ctx,
            actions_identifier,
            java_toolchain,
            srcs,
            remove_classes,
            annotation_processor_properties,
            plugin_params,
            source_level,
            target_level,
            deps,
            extra_arguments,
            additional_classpath_entries,
            bootclasspath_entries,
            compile_and_package_cmd,
            generated_sources_dir,
        )

    ctx.actions.run(compile_and_package_cmd, category = "javac_and_jar", identifier = actions_identifier)

    abi = None if (not srcs and not additional_compiled_srcs) or abi_generation_mode == AbiGenerationMode("none") or java_toolchain.is_bootstrap_toolchain else create_abi(ctx.actions, java_toolchain.class_abi_generator, jar_out)

    return make_compile_outputs(
        full_library = jar_out,
        class_abi = abi,
        required_for_source_only_abi = required_for_source_only_abi,
        annotation_processor_output = generated_sources_dir,
    )

def _check_dep_types(deps: list[Dependency]):
    for dep in deps:
        if JavaLibraryInfo not in dep and SharedLibraryInfo not in dep:
            fail("Received dependency {} is not supported. `java_library`, `prebuilt_jar` and native libraries are supported.".format(dep))

def _check_provided_deps(provided_deps: list[Dependency], attr_name: str):
    for provided_dep in provided_deps:
        expect(
            JavaLibraryInfo in provided_dep or SharedLibraryInfo not in provided_dep,
            "Java code does not need native libs in order to compile, so not valid as {}: {}".format(attr_name, provided_dep),
        )

def _check_exported_deps(exported_deps: list[Dependency], attr_name: str):
    for exported_dep in exported_deps:
        expect(
            JavaLibraryInfo in exported_dep,
            "Exported deps are meant to be forwarded onto the classpath for dependents, so only " +
            "make sense for a target that emits Java bytecode, {} in {} does not.".format(exported_dep, attr_name),
        )

# TODO(T145137403) remove need for this
def _skip_java_library_dep_checks(ctx: AnalysisContext) -> bool:
    return "skip_buck2_java_library_dep_checks" in ctx.attrs.labels

def java_library_impl(ctx: AnalysisContext) -> list[Provider]:
    """
     java_library() rule implementation

    Args:
        ctx: rule analysis context
    Returns:
        list of created providers
    """
    packaging_deps = ctx.attrs.deps + ctx.attrs.exported_deps + ctx.attrs.runtime_deps

    # TODO(T107163344) this shouldn't be in java_library itself, use overlays to remove it.
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

    if not _skip_java_library_dep_checks(ctx):
        _check_dep_types(ctx.attrs.deps)
        _check_dep_types(ctx.attrs.provided_deps)
        _check_dep_types(ctx.attrs.exported_deps)
        _check_dep_types(ctx.attrs.exported_provided_deps)
        _check_dep_types(ctx.attrs.runtime_deps)

    java_providers = build_java_library(ctx, ctx.attrs.srcs)

    return to_list(java_providers) + [android_packageable_info]

def build_java_library(
        ctx: AnalysisContext,
        srcs: list[Artifact],
        run_annotation_processors = True,
        additional_classpath_entries: list[Artifact] = [],
        bootclasspath_entries: list[Artifact] = [],
        additional_compiled_srcs: [Artifact, None] = None,
        generated_sources: list[Artifact] = [],
        override_abi_generation_mode: [AbiGenerationMode, None] = None,
        extra_sub_targets: dict = {}) -> JavaProviders:
    expect(
        not getattr(ctx.attrs, "_build_only_native_code", False),
        "Shouldn't call build_java_library if we're only building native code!",
    )

    _check_provided_deps(ctx.attrs.provided_deps, "provided_deps")
    _check_provided_deps(ctx.attrs.exported_provided_deps, "exported_provided_deps")
    _check_exported_deps(ctx.attrs.exported_deps, "exported_deps")
    _check_exported_deps(ctx.attrs.exported_provided_deps, "exported_provided_deps")

    deps_query = getattr(ctx.attrs, "deps_query", []) or []
    provided_deps_query = getattr(ctx.attrs, "provided_deps_query", []) or []
    first_order_deps = (
        ctx.attrs.deps +
        deps_query +
        ctx.attrs.exported_deps +
        ctx.attrs.provided_deps +
        provided_deps_query +
        ctx.attrs.exported_provided_deps
    )

    resources = ctx.attrs.resources
    resources_root = ctx.attrs.resources_root
    expect(resources_root != "", "Empty resources_root is not legal, try '.' instead!")

    annotation_processor_properties = create_annotation_processor_properties(
        ctx,
        ctx.attrs.plugins,
        ctx.attrs.annotation_processors,
        ctx.attrs.annotation_processor_params,
        ctx.attrs.annotation_processor_deps,
    ) if run_annotation_processors else None
    plugin_params = create_plugin_params(ctx, ctx.attrs.plugins) if run_annotation_processors else None
    manifest_file = ctx.attrs.manifest_file
    source_level, target_level = get_java_version_attributes(ctx)

    outputs = None
    common_compile_kwargs = None
    has_srcs = bool(srcs) or bool(additional_compiled_srcs)
    if has_srcs or resources or manifest_file:
        abi_generation_mode = override_abi_generation_mode or get_abi_generation_mode(ctx.attrs.abi_generation_mode)

        common_compile_kwargs = {
            "abi_generation_mode": abi_generation_mode,
            "additional_classpath_entries": additional_classpath_entries,
            "additional_compiled_srcs": additional_compiled_srcs,
            "annotation_processor_properties": annotation_processor_properties,
            "bootclasspath_entries": bootclasspath_entries,
            "deps": first_order_deps,
            "javac_tool": derive_javac(ctx.attrs.javac) if ctx.attrs.javac else None,
            "manifest_file": manifest_file,
            "remove_classes": ctx.attrs.remove_classes,
            "required_for_source_only_abi": ctx.attrs.required_for_source_only_abi,
            "resources": resources,
            "resources_root": resources_root,
            "source_level": source_level,
            "source_only_abi_deps": ctx.attrs.source_only_abi_deps,
            "srcs": srcs,
            "target_level": target_level,
        }

        outputs = compile_to_jar(
            ctx,
            plugin_params = plugin_params,
            extra_arguments = cmd_args(ctx.attrs.extra_arguments),
            **common_compile_kwargs
        )

    java_toolchain = ctx.attrs._java_toolchain[JavaToolchainInfo]
    if (
        common_compile_kwargs and
        srcs and
        not java_toolchain.is_bootstrap_toolchain and
        not ctx.attrs._is_building_android_binary
    ):
        nullsafe_info = get_nullsafe_info(ctx)
        if nullsafe_info:
            compile_to_jar(
                ctx,
                actions_identifier = "nullsafe",
                plugin_params = nullsafe_info.plugin_params,
                extra_arguments = nullsafe_info.extra_arguments,
                is_creating_subtarget = True,
                **common_compile_kwargs
            )

            extra_sub_targets = extra_sub_targets | {"nullsafex-json": [
                DefaultInfo(default_output = nullsafe_info.output),
            ]}

    all_generated_sources = list(generated_sources)
    if outputs and outputs.annotation_processor_output:
        all_generated_sources.append(outputs.annotation_processor_output)

    if len(all_generated_sources) == 1:
        extra_sub_targets = extra_sub_targets | {"generated_sources": [
            DefaultInfo(default_output = all_generated_sources[0]),
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
        generated_sources = all_generated_sources,
        has_srcs = has_srcs,
    )

    class_to_src_map, class_to_src_map_sub_targets = get_class_to_source_map_info(
        ctx,
        outputs = outputs,
        deps = ctx.attrs.deps + deps_query + ctx.attrs.exported_deps,
    )
    extra_sub_targets = extra_sub_targets | class_to_src_map_sub_targets

    default_info = get_default_info(
        ctx.actions,
        java_toolchain,
        outputs,
        java_packaging_info,
        extra_sub_targets,
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
