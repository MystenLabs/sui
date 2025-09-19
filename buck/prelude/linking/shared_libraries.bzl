# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//cxx:cxx_toolchain_types.bzl", "CxxToolchainInfo")
load(
    "@prelude//linking:link_info.bzl",
    "LinkArgs",
    "LinkedObject",  # @unused Used as a type
)
load("@prelude//linking:strip.bzl", "strip_object")

SharedLibrary = record(
    lib = field(LinkedObject),
    # The LinkArgs used to produce this SharedLibrary. This can be useful for debugging or
    # for downstream rules to reproduce the shared library with some modifications (for example
    # android relinker will link again with an added version script argument).
    # TODO(cjhopman): This is currently always available.
    link_args = field(list[LinkArgs] | None),
    # The sonames of the shared libraries that this links against.
    # TODO(cjhopman): This is currently always available.
    shlib_deps = field(list[str] | None),
    stripped_lib = field([Artifact, None]),
    can_be_asset = field(bool),
    for_primary_apk = field(bool),
    soname = field(str),
    label = field(Label),
)

SharedLibraries = record(
    # A mapping of shared library SONAME (e.g. `libfoo.so.2`) to the artifact.
    # Since the SONAME is what the dynamic loader uses to uniquely identify
    # libraries, using this as the key allows easily detecting conflicts from
    # dependencies.
    libraries = field(dict[str, SharedLibrary]),
)

# T-set of SharedLibraries
SharedLibrariesTSet = transitive_set()

# Shared libraries required by top-level packaging rules (e.g. shared libs
# for Python binary, symlink trees of shared libs for C++ binaries)
SharedLibraryInfo = provider(fields = {
    "set": provider_field(typing.Any, default = None),  # SharedLibrariesTSet | None
})

def get_strip_non_global_flags(cxx_toolchain: CxxToolchainInfo) -> list:
    if cxx_toolchain.strip_flags_info and cxx_toolchain.strip_flags_info.strip_non_global_flags:
        return cxx_toolchain.strip_flags_info.strip_non_global_flags

    return ["--strip-unneeded"]

def create_shared_libraries(
        ctx: AnalysisContext,
        libraries: dict[str, LinkedObject]) -> SharedLibraries:
    """
    Take a mapping of dest -> src and turn it into a mapping that will be
    passed around in providers. Used for both srcs, and resources.
    """
    cxx_toolchain = getattr(ctx.attrs, "_cxx_toolchain", None)
    return SharedLibraries(
        libraries = {name: SharedLibrary(
            lib = shlib,
            stripped_lib = strip_object(
                ctx,
                cxx_toolchain[CxxToolchainInfo],
                shlib.output,
                cmd_args(get_strip_non_global_flags(cxx_toolchain[CxxToolchainInfo])),
            ) if cxx_toolchain != None else None,
            link_args = shlib.link_args,
            shlib_deps = None,  # TODO(cjhopman): we need this figured out.
            can_be_asset = getattr(ctx.attrs, "can_be_asset", False) or False,
            for_primary_apk = getattr(ctx.attrs, "used_by_wrap_script", False),
            label = ctx.label,
            soname = name,
        ) for (name, shlib) in libraries.items()},
    )

# We do a lot of merging library maps, so don't use O(n) type annotations
def _merge_lib_map(
        # dict[str, SharedLibrary]
        dest_mapping,
        # [dict[str, SharedLibrary]
        mapping_to_merge,
        filter_func) -> None:
    """
    Merges a mapping_to_merge into `dest_mapping`. Fails if different libraries
    map to the same name.
    """
    for (name, src) in mapping_to_merge.items():
        if filter_func != None and not filter_func(name, src):
            continue
        existing = dest_mapping.get(name)
        if existing != None and existing.lib != src.lib:
            error = (
                "Duplicate library {}! Conflicting mappings:\n" +
                "{} from {}\n" +
                "{} from {}"
            )
            fail(
                error.format(
                    name,
                    existing.lib,
                    existing.label,
                    src.lib,
                    src.label,
                ),
            )
        dest_mapping[name] = src

# Merge multiple SharedLibraryInfo. The value in `node` represents a set of
# SharedLibraries that is provided by the target being analyzed. It's optional
# because that might not always exist, e.g. a Python library can pass through
# SharedLibraryInfo but it cannot produce any. The value in `deps` represents
# all the inherited shared libraries for this target.
def merge_shared_libraries(
        actions: AnalysisActions,
        node: [SharedLibraries, None] = None,
        deps: list[SharedLibraryInfo] = []) -> SharedLibraryInfo:
    kwargs = {}

    children = filter(None, [dep.set for dep in deps])
    if children:
        kwargs["children"] = children
    if node:
        kwargs["value"] = node

    set = actions.tset(SharedLibrariesTSet, **kwargs) if kwargs else None
    return SharedLibraryInfo(set = set)

def traverse_shared_library_info(
        info: SharedLibraryInfo,
        filter_func = None):  # -> dict[str, SharedLibrary]:
    libraries = {}
    if info.set:
        for libs in info.set.traverse():
            _merge_lib_map(libraries, libs.libraries, filter_func)
    return libraries
