# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//cxx:cxx_library_utility.bzl", "cxx_inherited_link_info")
load(
    "@prelude//cxx:cxx_link_utility.bzl",
    "executable_shared_lib_arguments",
    "make_link_args",
)
load(
    "@prelude//linking:link_info.bzl",
    "LinkStyle",
    "get_link_args_for_strategy",
    "to_link_strategy",
)
load(
    "@prelude//linking:shared_libraries.bzl",
    "SharedLibraryInfo",
    "merge_shared_libraries",
    "traverse_shared_library_info",
)
load("@prelude//os_lookup:defs.bzl", "OsLookup")
load(
    "@prelude//utils:utils.bzl",
    "map_idx",
)
load(
    ":packages.bzl",
    "GoPkg",  # @Unused used as type
    "merge_pkgs",
    "pkg_artifacts",
    "stdlib_pkg_artifacts",
)
load(":toolchain.bzl", "GoToolchainInfo", "get_toolchain_cmd_args")

# Provider wrapping packages used for linking.
GoPkgLinkInfo = provider(fields = {
    "pkgs": provider_field(typing.Any, default = None),  # {str: "artifact"}
})

GoBuildMode = enum(
    "executable",
    "c_shared",
)

def _build_mode_param(mode: GoBuildMode) -> str:
    if mode == GoBuildMode("executable"):
        return "exe"
    if mode == GoBuildMode("c_shared"):
        return "c-shared"
    fail("unexpected: {}", mode)

def get_inherited_link_pkgs(deps: list[Dependency]) -> dict[str, GoPkg]:
    return merge_pkgs([d[GoPkgLinkInfo].pkgs for d in deps if GoPkgLinkInfo in d])

def is_any_dep_cgo(deps: list[Dependency]) -> bool:
    for d in deps:
        if GoPkgLinkInfo in d:
            for pkg in d[GoPkgLinkInfo].pkgs.values():
                if pkg.cgo:
                    return True
    return False

# TODO(cjhopman): Is link_style a LibOutputStyle or a LinkStrategy here? Based on returning an empty thing for link_style != shared,
# it seems likely its intended to be LibOutputStyle, but it's called in places that are passing what appears to be a LinkStrategy.
def _process_shared_dependencies(ctx: AnalysisContext, artifact: Artifact, deps: list[Dependency], link_style: LinkStyle):
    """
    Provides files and linker args needed to for binaries with shared library linkage.
    - the runtime files needed to run binary linked with shared libraries
    - linker arguments for shared libraries
    """
    if link_style != LinkStyle("shared"):
        return ([], [])

    shlib_info = merge_shared_libraries(
        ctx.actions,
        deps = filter(None, map_idx(SharedLibraryInfo, deps)),
    )
    shared_libs = {}
    for name, shared_lib in traverse_shared_library_info(shlib_info).items():
        shared_libs[name] = shared_lib.lib

    extra_link_args, runtime_files, _ = executable_shared_lib_arguments(
        ctx.actions,
        ctx.attrs._go_toolchain[GoToolchainInfo].cxx_toolchain_for_linking,
        artifact,
        shared_libs,
    )

    return (runtime_files, extra_link_args)

def link(
        ctx: AnalysisContext,
        main: Artifact,
        pkgs: dict[str, Artifact] = {},
        deps: list[Dependency] = [],
        build_mode: GoBuildMode = GoBuildMode("executable"),
        link_mode: [str, None] = None,
        link_style: LinkStyle = LinkStyle("static"),
        linker_flags: list[typing.Any] = [],
        external_linker_flags: list[typing.Any] = [],
        shared: bool = False):
    go_toolchain = ctx.attrs._go_toolchain[GoToolchainInfo]
    if go_toolchain.env_go_os == "windows":
        executable_extension = ".exe"
        shared_extension = ".dll"
    else:
        executable_extension = ""
        shared_extension = ".so"
    file_extension = shared_extension if build_mode == GoBuildMode("c_shared") else executable_extension
    output = ctx.actions.declare_output(ctx.label.name + file_extension)

    cmd = get_toolchain_cmd_args(go_toolchain)

    cmd.add(go_toolchain.linker)
    if shared:
        cmd.add(go_toolchain.linker_flags_shared)
    else:
        cmd.add(go_toolchain.linker_flags_static)

    cmd.add("-o", output.as_output())
    cmd.add("-buildmode=" + _build_mode_param(build_mode))
    cmd.add("-buildid=")  # Setting to a static buildid helps make the binary reproducible.

    # Add inherited Go pkgs to library search path.
    all_pkgs = merge_pkgs([
        pkgs,
        pkg_artifacts(get_inherited_link_pkgs(deps), shared = shared),
        stdlib_pkg_artifacts(go_toolchain, shared = shared),
    ])

    importcfg_content = []
    for name_, pkg_ in all_pkgs.items():
        # Hack: we use cmd_args get "artifact" valid path and write it to a file.
        importcfg_content.append(cmd_args("packagefile ", name_, "=", pkg_, delimiter = ""))

    importcfg = ctx.actions.declare_output("importcfg")
    ctx.actions.write(importcfg.as_output(), importcfg_content)

    cmd.add("-importcfg", importcfg)
    cmd.hidden(all_pkgs.values())

    runtime_files, extra_link_args = _process_shared_dependencies(ctx, output, deps, link_style)

    if link_mode == None:
        if build_mode == GoBuildMode("c_shared"):
            link_mode = "external"
        elif shared:
            link_mode = "external"
        elif is_any_dep_cgo(deps):
            link_mode = "external"
        else:
            link_mode = "internal"
    cmd.add("-linkmode", link_mode)

    if link_mode == "external":
        is_win = ctx.attrs._exec_os_type[OsLookup].platform == "windows"
        cxx_toolchain = go_toolchain.cxx_toolchain_for_linking

        # Gather external link args from deps.
        ext_links = get_link_args_for_strategy(ctx, cxx_inherited_link_info(deps), to_link_strategy(link_style))
        ext_link_args_output = make_link_args(
            ctx.actions,
            cxx_toolchain,
            [ext_links],
        )
        ext_link_args = cmd_args()
        ext_link_args.add(cmd_args(extra_link_args, quote = "shell"))
        ext_link_args.add(external_linker_flags)
        ext_link_args.add(ext_link_args_output.link_args)
        ext_link_args.hidden(ext_link_args_output.hidden)

        # Delegate to C++ linker...
        # TODO: It feels a bit inefficient to generate a wrapper file for every
        # link.  Is there some way to etract the first arg of `RunInfo`?  Or maybe
        # we can generate te platform-specific stuff once and re-use?
        cxx_link_cmd = cmd_args(
            [
                cxx_toolchain.linker_info.linker,
                cxx_toolchain.linker_info.linker_flags,
                go_toolchain.external_linker_flags,
                ext_link_args,
                "%*" if is_win else "\"$@\"",
            ],
            delimiter = " ",
        )
        linker_wrapper, _ = ctx.actions.write(
            "__{}_cxx_link_wrapper__.{}".format(ctx.label.name, "bat" if is_win else "sh"),
            ([] if is_win else ["#!/bin/sh"]) + [cxx_link_cmd],
            allow_args = True,
            is_executable = True,
        )
        cmd.add("-extld", linker_wrapper).hidden(cxx_link_cmd)

    cmd.add(linker_flags)

    cmd.add(main)

    ctx.actions.run(cmd, category = "go_link")

    return (output, runtime_files)
