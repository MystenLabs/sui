# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(
    "@prelude//linking:shared_libraries.bzl",
    "SharedLibraryInfo",  # @unused Used as a type
    "merge_shared_libraries",
)

JuliaToolchainInfo = provider(
    # @unsorted-dict-items
    fields = {
        "julia": provider_field(typing.Any, default = None),
        "env": provider_field(typing.Any, default = None),
        "cmd_processor": provider_field(typing.Any, default = None),
    },
)

JllInfo = record(
    name = field(str),
    libs = field(dict),  # Julia name to label
)

JuliaLibrary = record(
    uuid = str,
    src_labels = typing.Any,
    srcs = typing.Any,
    project_toml = typing.Any,
    label = field(Label),
    jll = field([JllInfo, None]),
)

def project_load_src_label(lib):
    return lib.src_labels

def project_load_srcs(lib):
    return lib.srcs

JuliaLibraryTSet = transitive_set(
    args_projections = {
        "load_src_label": project_load_src_label,
        "load_srcs": project_load_srcs,
    },
)

# Information about a julia library and its dependencies.
JuliaLibraryInfo = provider(fields = {
    "julia_tsets": provider_field(typing.Any, default = None),  # JuliaLibraryTSet
    "shared_library_info": provider_field(typing.Any, default = None),  # SharedLibraryInfo
})

def create_julia_library_info(
        actions: AnalysisActions,
        label: Label,
        uuid: str = "",
        src_labels: typing.Any = [],
        project_toml: typing.Any = None,
        srcs: typing.Any = [],
        deps: list[JuliaLibraryInfo] = [],
        jll: [JllInfo, None] = None,
        shlibs: list[SharedLibraryInfo] = []) -> JuliaLibraryInfo:
    julia_tsets = JuliaLibrary(
        uuid = uuid,
        label = label,
        src_labels = src_labels,
        srcs = srcs,
        project_toml = project_toml,
        jll = jll,
    )

    return JuliaLibraryInfo(
        julia_tsets = actions.tset(JuliaLibraryTSet, value = julia_tsets, children = [dep.julia_tsets for dep in deps]),
        shared_library_info = merge_shared_libraries(
            actions,
            None,
            [dep.shared_library_info for dep in deps] + shlibs,
        ),
    )
