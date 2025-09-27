# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//java:java_providers.bzl", "JavaLibraryInfo", "JavaPackagingDepTSet", "JavaPackagingInfo")

JavaProcessorsType = enum(
    "java_annotation_processor",
    "ksp_annotation_processor",
    "plugin",
)

JavaProcessorsInfo = provider(
    # @unsorted-dict-items
    doc = "Information about java annotation processor/ java compiler plugins and their dependencies",
    fields = {
        "affects_abi": provider_field(typing.Any, default = None),

        # Java dependencies exposed to dependent targets and supposed to be used during compilation.
        "deps": provider_field(typing.Any, default = None),  # [JavaPackagingDepTSet, None]
        "isolate_class_loader": provider_field(typing.Any, default = None),

        # Names of processors
        "processors": provider_field(typing.Any, default = None),  # ["string"]
        "supports_source_only_abi": provider_field(typing.Any, default = None),
        # Type of processor
        "type": provider_field(typing.Any, default = None),  # "JavaProcessorsType"
    },
)

AnnotationProcessor = record(
    affects_abi = field(bool),
    supports_source_only_abi = field(bool),
    processors = field(list[str]),
    deps = field([JavaPackagingDepTSet, None]),
    isolate_class_loader = field(bool),
)

AnnotationProcessorProperties = record(
    annotation_processors = field(list[AnnotationProcessor]),
    annotation_processor_params = field(list[str]),
)

# Every transitive java annotation processors dependency has to be included into processor classpath for AP/Java Plugin run
def derive_transitive_deps(ctx: AnalysisContext, deps: list[Dependency]) -> [JavaPackagingDepTSet, None]:
    for dep in deps:
        if not dep[JavaLibraryInfo]:
            fail("Dependency must have a type of `java_library` or `prebuilt_jar`. Deps: {}".format(deps))

    return ctx.actions.tset(
        JavaPackagingDepTSet,
        children = [dep[JavaPackagingInfo].packaging_deps for dep in deps],
    ) if deps else None

def create_annotation_processor_properties(
        ctx: AnalysisContext,
        plugins: list[Dependency],
        annotation_processor_names: list[str],
        annotation_processor_params: list[str],
        annotation_processor_deps: list[Dependency]) -> AnnotationProcessorProperties:
    annotation_processors = []

    # Extend `ap_processor_deps` with java deps from `annotation_processor_deps`
    if annotation_processor_names or annotation_processor_deps:
        for ap_dep in [x.get(JavaLibraryInfo) for x in annotation_processor_deps]:
            if not ap_dep:
                fail("Dependency must have a type of `java_library` or `prebuilt_jar`. Deps: {}".format(annotation_processor_deps))

        # "legacy" annotation processors have no mechanism for indicating if they affect abi or if they support source_only
        annotation_processors.append(AnnotationProcessor(
            affects_abi = True,
            supports_source_only_abi = False,
            processors = annotation_processor_names,
            # using packaging deps to have all transitive deps collected for processors classpath
            deps = derive_transitive_deps(ctx, annotation_processor_deps),
            isolate_class_loader = False,
        ))

    # APs derived from `plugins` attribute
    for ap_plugin in filter(None, [x.get(JavaProcessorsInfo) for x in plugins]):
        if not ap_plugin:
            fail("Plugin must have a type of `java_annotation_processor` or `java_plugin`. Plugins: {}".format(plugins))
        if ap_plugin.type == JavaProcessorsType("java_annotation_processor"):
            annotation_processors.append(AnnotationProcessor(
                affects_abi = ap_plugin.affects_abi,
                supports_source_only_abi = ap_plugin.supports_source_only_abi,
                processors = ap_plugin.processors,
                deps = ap_plugin.deps,
                isolate_class_loader = ap_plugin.isolate_class_loader,
            ))

    return AnnotationProcessorProperties(
        annotation_processors = annotation_processors,
        annotation_processor_params = annotation_processor_params,
    )

def create_ksp_annotation_processor_properties(ctx: AnalysisContext, plugins: list[Dependency]) -> AnnotationProcessorProperties:
    ap_processors = []
    ap_processor_deps = []

    # APs derived from `plugins` attribute
    for ap_plugin in filter(None, [x.get(JavaProcessorsInfo) for x in plugins]):
        if not ap_plugin:
            fail("Plugin must have a type of `java_annotation_processor` or `java_plugin`. Plugins: {}".format(plugins))
        if ap_plugin.type == JavaProcessorsType("ksp_annotation_processor"):
            ap_processors += ap_plugin.processors
            if ap_plugin.deps:
                ap_processor_deps.append(ap_plugin.deps)

    if not ap_processors:
        return AnnotationProcessorProperties(annotation_processors = [], annotation_processor_params = [])

    return AnnotationProcessorProperties(
        annotation_processors = [AnnotationProcessor(
            processors = dedupe(ap_processors),
            deps = ctx.actions.tset(JavaPackagingDepTSet, children = ap_processor_deps) if ap_processor_deps else None,
            affects_abi = True,
            supports_source_only_abi = False,
            isolate_class_loader = False,
        )],
        annotation_processor_params = [],
    )

def _get_processor_type(processor_class: str) -> JavaProcessorsType:
    if processor_class.startswith("KSP:"):
        return JavaProcessorsType("ksp_annotation_processor")

    return JavaProcessorsType("java_annotation_processor")

def java_annotation_processor_impl(ctx: AnalysisContext) -> list[Provider]:
    if ctx.attrs._build_only_native_code:
        return [DefaultInfo()]

    transitive_deps = derive_transitive_deps(ctx, ctx.attrs.deps)

    return [
        JavaProcessorsInfo(
            deps = transitive_deps,
            processors = [ctx.attrs.processor_class],
            type = _get_processor_type(ctx.attrs.processor_class),
            affects_abi = not ctx.attrs.does_not_affect_abi,
            supports_source_only_abi = ctx.attrs.supports_abi_generation_from_source,
            isolate_class_loader = ctx.attrs.isolate_class_loader,
        ),
        DefaultInfo(default_output = None, other_outputs = [packaging_dep.jar for packaging_dep in transitive_deps.traverse() if packaging_dep.jar]),
    ]
