# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(
    "@prelude//java:java_toolchain.bzl",
    "JavaTestToolchainInfo",  # @unused Used as a type
    "JavaToolchainInfo",  # @unused Used as a type
)

def _class_to_src_map_args(mapping: [Artifact, None]):
    if mapping != None:
        return cmd_args(mapping)
    return cmd_args()

JavaClassToSourceMapTset = transitive_set(
    args_projections = {
        "class_to_src_map": _class_to_src_map_args,
    },
)

JavaClassToSourceMapInfo = provider(
    # @unsorted-dict-items
    fields = {
        "tset": provider_field(typing.Any, default = None),
        "debuginfo": provider_field(typing.Any, default = None),
        # Used internally in this module to aid generation of `debuginfo`.
        "_tset_debuginfo": provider_field(typing.Any, default = None),
    },
)

def create_class_to_source_map_info(
        ctx: AnalysisContext,
        mapping: [Artifact, None] = None,
        mapping_debuginfo: [Artifact, None] = None,
        deps = [Dependency]) -> JavaClassToSourceMapInfo:
    # Only generate debuginfo if the debug info tool is available.
    java_toolchain = ctx.attrs._java_toolchain[JavaToolchainInfo]
    tset_debuginfo = None
    debuginfo = None
    if java_toolchain.gen_class_to_source_map_debuginfo != None:
        tset_debuginfo = ctx.actions.tset(
            JavaClassToSourceMapTset,
            value = mapping_debuginfo,
            children = [
                d[JavaClassToSourceMapInfo]._tset_debuginfo
                for d in deps
                if JavaClassToSourceMapInfo in d and d[JavaClassToSourceMapInfo]._tset_debuginfo != None
            ],
        )
        debuginfo = _create_merged_debug_info(
            actions = ctx.actions,
            java_toolchain = ctx.attrs._java_toolchain[JavaToolchainInfo],
            tset_debuginfo = tset_debuginfo,
            name = ctx.attrs.name + ".debuginfo_merged.json",
        )

    return JavaClassToSourceMapInfo(
        _tset_debuginfo = tset_debuginfo,
        tset = ctx.actions.tset(
            JavaClassToSourceMapTset,
            value = mapping,
            children = [d[JavaClassToSourceMapInfo].tset for d in deps if JavaClassToSourceMapInfo in d],
        ),
        debuginfo = debuginfo,
    )

def create_class_to_source_map_from_jar(
        actions: AnalysisActions,
        name: str,
        java_toolchain: JavaToolchainInfo,
        jar: Artifact,
        srcs: list[Artifact]) -> Artifact:
    output = actions.declare_output(name)
    cmd = cmd_args(java_toolchain.gen_class_to_source_map[RunInfo])
    cmd.add("-o", output.as_output())
    cmd.add(jar)
    for src in srcs:
        cmd.add(cmd_args(src))
    actions.run(cmd, category = "class_to_srcs_map")
    return output

def maybe_create_class_to_source_map_debuginfo(
        actions: AnalysisActions,
        name: str,
        java_toolchain: JavaToolchainInfo,
        srcs: list[Artifact]) -> [Artifact, None]:
    # Only generate debuginfo if the debug info tool is available.
    if java_toolchain.gen_class_to_source_map_debuginfo == None:
        return None

    output = actions.declare_output(name)
    cmd = cmd_args(java_toolchain.gen_class_to_source_map_debuginfo[RunInfo])
    cmd.add("gen")
    cmd.add("-o", output.as_output())
    inputs_file = actions.write("sourcefiles.txt", srcs)
    cmd.add(cmd_args(inputs_file, format = "@{}"))
    cmd.hidden(srcs)
    actions.run(cmd, category = "class_to_srcs_map_debuginfo")
    return output

def merge_class_to_source_map_from_jar(
        actions: AnalysisActions,
        name: str,
        java_test_toolchain: JavaTestToolchainInfo,
        mapping: [Artifact, None] = None,
        relative_to: [CellRoot, None] = None,
        # TODO(nga): I think this meant to be type, not default value.
        deps = [JavaClassToSourceMapInfo.type]) -> Artifact:
    output = actions.declare_output(name)
    cmd = cmd_args(java_test_toolchain.merge_class_to_source_maps[RunInfo])
    cmd.add(cmd_args(output.as_output(), format = "--output={}"))
    if relative_to != None:
        cmd.add(cmd_args(str(relative_to), format = "--relative-to={}"))
    tset = actions.tset(
        JavaClassToSourceMapTset,
        value = mapping,
        children = [d.tset for d in deps],
    )
    class_to_source_files = tset.project_as_args("class_to_src_map")
    mappings_file = actions.write("class_to_src_map.txt", class_to_source_files)
    cmd.add(["--mappings", mappings_file])
    cmd.hidden(class_to_source_files)
    actions.run(cmd, category = "merge_class_to_srcs_map")
    return output

def _create_merged_debug_info(
        actions: AnalysisActions,
        java_toolchain: JavaToolchainInfo,
        tset_debuginfo: TransitiveSet,
        name: str):
    output = actions.declare_output(name)
    cmd = cmd_args(java_toolchain.gen_class_to_source_map_debuginfo[RunInfo])
    cmd.add("merge")
    cmd.add(cmd_args(output.as_output(), format = "-o={}"))

    tset = actions.tset(
        JavaClassToSourceMapTset,
        children = [tset_debuginfo],
    )
    input_files = tset.project_as_args("class_to_src_map")
    input_list_file = actions.write("debuginfo_list.txt", input_files)
    cmd.add(cmd_args(input_list_file, format = "@{}"))
    cmd.hidden(input_files)
    actions.run(cmd, category = "merged_debuginfo")
    return output
