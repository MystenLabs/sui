# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# Implementation of the Haskell build rules.

load("@prelude//:paths.bzl", "paths")
load("@prelude//cxx:archive.bzl", "make_archive")
load(
    "@prelude//cxx:cxx.bzl",
    "get_auto_link_group_specs",
)
load(
    "@prelude//cxx:cxx_toolchain_types.bzl",
    "CxxPlatformInfo",
    "CxxToolchainInfo",
    "PicBehavior",
)
load(
    "@prelude//cxx:link_groups.bzl",
    "LinkGroupContext",
    "create_link_groups",
    "find_relevant_roots",
    "get_filtered_labels_to_links_map",
    "get_filtered_links",
    "get_link_group_info",
    "get_link_group_preferred_linkage",
    "get_transitive_deps_matching_labels",
    "is_link_group_shlib",
)
load(
    "@prelude//cxx:preprocessor.bzl",
    "CPreprocessor",
    "CPreprocessorArgs",
    "cxx_inherited_preprocessor_infos",
    "cxx_merge_cpreprocessors",
)
load(
    "@prelude//linking:link_groups.bzl",
    "gather_link_group_libs",
    "merge_link_group_lib_info",
)
load(
    "@prelude//linking:link_info.bzl",
    "Archive",
    "ArchiveLinkable",
    "LibOutputStyle",
    "LinkArgs",
    "LinkInfo",
    "LinkInfos",
    "LinkStyle",
    "Linkage",
    "LinkedObject",
    "MergedLinkInfo",
    "SharedLibLinkable",
    "create_merged_link_info",
    "default_output_style_for_link_strategy",
    "get_lib_output_style",
    "get_link_args_for_strategy",
    "get_output_styles_for_linkage",
    "legacy_output_style_to_link_style",
    "to_link_strategy",
    "unpack_link_args",
)
load(
    "@prelude//linking:linkable_graph.bzl",
    "LinkableGraph",
    "create_linkable_graph",
    "create_linkable_graph_node",
    "create_linkable_node",
    "get_linkable_graph_node_map_func",
)
load(
    "@prelude//linking:linkables.bzl",
    "linkables",
)
load(
    "@prelude//linking:shared_libraries.bzl",
    "SharedLibraryInfo",
    "create_shared_libraries",
    "merge_shared_libraries",
    "traverse_shared_library_info",
)
load(
    "@prelude//python:python.bzl",
    "PythonLibraryInfo",
)
load("@prelude//utils:platform_flavors_util.bzl", "by_platform")
load("@prelude//utils:set.bzl", "set")
load("@prelude//utils:utils.bzl", "filter_and_map_idx", "flatten")

_HASKELL_EXTENSIONS = [
    ".hs",
    ".lhs",
    ".hsc",
    ".chs",
    ".x",
    ".y",
]

HaskellPlatformInfo = provider(fields = {
    "name": provider_field(typing.Any, default = None),
})

HaskellToolchainInfo = provider(
    # @unsorted-dict-items
    fields = {
        "compiler": provider_field(typing.Any, default = None),
        "compiler_flags": provider_field(typing.Any, default = None),
        "linker": provider_field(typing.Any, default = None),
        "linker_flags": provider_field(typing.Any, default = None),
        "haddock": provider_field(typing.Any, default = None),
        "compiler_major_version": provider_field(typing.Any, default = None),
        "package_name_prefix": provider_field(typing.Any, default = None),
        "packager": provider_field(typing.Any, default = None),
        "use_argsfile": provider_field(typing.Any, default = None),
        "support_expose_package": provider_field(bool, default = False),
        "archive_contents": provider_field(typing.Any, default = None),
        "ghci_script_template": provider_field(typing.Any, default = None),
        "ghci_iserv_template": provider_field(typing.Any, default = None),
        "ide_script_template": provider_field(typing.Any, default = None),
        "ghci_binutils_path": provider_field(typing.Any, default = None),
        "ghci_lib_path": provider_field(typing.Any, default = None),
        "ghci_ghc_path": provider_field(typing.Any, default = None),
        "ghci_iserv_path": provider_field(typing.Any, default = None),
        "ghci_iserv_prof_path": provider_field(typing.Any, default = None),
        "ghci_cxx_path": provider_field(typing.Any, default = None),
        "ghci_cc_path": provider_field(typing.Any, default = None),
        "ghci_cpp_path": provider_field(typing.Any, default = None),
        "ghci_packager": provider_field(typing.Any, default = None),
        "cache_links": provider_field(typing.Any, default = None),
        "script_template_processor": provider_field(typing.Any, default = None),
    },
)

# A list of `HaskellLibraryInfo`s.
HaskellLinkInfo = provider(
    # Contains a list of HaskellLibraryInfo records.
    fields = {
        "info": provider_field(typing.Any, default = None),  # dict[LinkStyle, list[HaskellLibraryInfo]] # TODO use a tset
        "prof_info": provider_field(typing.Any, default = None),  # dict[LinkStyle, list[HaskellLibraryInfo]] # TODO use a tset
    },
)

HaskellIndexingTSet = transitive_set()

# A list of hie dirs
HaskellIndexInfo = provider(
    fields = {
        "info": provider_field(typing.Any, default = None),  # dict[LinkStyle, HaskellIndexingTset]
    },
)

# If the target is a haskell library, the HaskellLibraryProvider
# contains its HaskellLibraryInfo. (in contrast to a HaskellLinkInfo,
# which contains the HaskellLibraryInfo for all the transitive
# dependencies). Direct dependencies are treated differently from
# indirect dependencies for the purposes of module visibility.
HaskellLibraryProvider = provider(
    fields = {
        "lib": provider_field(typing.Any, default = None),  # dict[LinkStyle, HaskellLibraryInfo]
        "prof_lib": provider_field(typing.Any, default = None),  # dict[LinkStyle, HaskellLibraryInfo]
    },
)

# HaskellProfLinkInfo exposes the MergedLinkInfo of a target and all of its
# dependencies built for profiling. This allows top-level targets (e.g.
# `haskell_binary`) to be defined with profiling enabled by default.
HaskellProfLinkInfo = provider(
    fields = {
        "prof_infos": provider_field(typing.Any, default = None),  # MergedLinkInfo
    },
)

# A record of a Haskell library.
HaskellLibraryInfo = record(
    # The library target name: e.g. "rts"
    name = str,
    # package config database: e.g. platform009/build/ghc/lib/package.conf.d
    db = Artifact,
    # e.g. "base-4.13.0.0"
    id = str,
    # Import dirs indexed by profiling enabled/disabled
    import_dirs = dict[bool, Artifact],
    stub_dirs = list[Artifact],

    # This field is only used as hidden inputs to compilation, to
    # support Template Haskell which may need access to the libraries
    # at compile time.  The real library flags are propagated up the
    # dependency graph via MergedLinkInfo.
    libs = field(list[Artifact], []),
    # Package version, used to specify the full package when exposing it,
    # e.g. filepath-1.4.2.1, deepseq-1.4.4.0.
    # Internal packages default to 1.0.0, e.g. `fbcode-dsi-logger-hs-types-1.0.0`.
    version = str,
    is_prebuilt = bool,
    profiling_enabled = bool,
)

# --

def _by_platform(ctx: AnalysisContext, xs: list[(str, list[typing.Any])]) -> list[typing.Any]:
    platform = ctx.attrs._cxx_toolchain[CxxPlatformInfo].name
    return flatten(by_platform([platform], xs))

def attr_deps(ctx: AnalysisContext) -> list[Dependency]:
    return ctx.attrs.deps + _by_platform(ctx, ctx.attrs.platform_deps)

# Disable until we have a need to call this.
# def _attr_deps_merged_link_infos(ctx: AnalysisContext) -> [MergedLinkInfo]:
#     return filter(None, [d[MergedLinkInfo] for d in attr_deps(ctx)])

# This conversion is non-standard, see TODO about link style below
def _to_lib_output_style(link_style: LinkStyle) -> LibOutputStyle:
    return default_output_style_for_link_strategy(to_link_strategy(link_style))

def _attr_deps_haskell_link_infos(ctx: AnalysisContext) -> list[HaskellLinkInfo]:
    return filter(
        None,
        [
            d.get(HaskellLinkInfo)
            for d in attr_deps(ctx) + ctx.attrs.template_deps
        ],
    )

def _attr_deps_haskell_lib_infos(
        ctx: AnalysisContext,
        link_style: LinkStyle,
        enable_profiling: bool) -> list[HaskellLibraryInfo]:
    if enable_profiling and link_style == LinkStyle("shared"):
        fail("Profiling isn't supported when using dynamic linking")
    return [
        x.prof_lib[link_style] if enable_profiling else x.lib[link_style]
        for x in filter(None, [
            d.get(HaskellLibraryProvider)
            for d in attr_deps(ctx) + ctx.attrs.template_deps
        ])
    ]

def _cxx_toolchain_link_style(ctx: AnalysisContext) -> LinkStyle:
    return ctx.attrs._cxx_toolchain[CxxToolchainInfo].linker_info.link_style

def _attr_link_style(ctx: AnalysisContext) -> LinkStyle:
    if ctx.attrs.link_style != None:
        return LinkStyle(ctx.attrs.link_style)
    else:
        return _cxx_toolchain_link_style(ctx)

def _attr_preferred_linkage(ctx: AnalysisContext) -> Linkage:
    preferred_linkage = ctx.attrs.preferred_linkage

    # force_static is deprecated, but it has precedence over preferred_linkage
    if getattr(ctx.attrs, "force_static", False):
        preferred_linkage = "static"

    return Linkage(preferred_linkage)

# --

def _is_haskell_src(x: str) -> bool:
    _, ext = paths.split_extension(x)
    return ext in _HASKELL_EXTENSIONS

def _src_to_module_name(x: str) -> str:
    base, _ext = paths.split_extension(x)
    return base.replace("/", ".")

def _get_haskell_prebuilt_libs(
        ctx,
        link_style: LinkStyle,
        enable_profiling: bool) -> list[Artifact]:
    if link_style == LinkStyle("shared"):
        if enable_profiling:
            # Profiling doesn't support shared libraries
            return []

        return ctx.attrs.shared_libs.values()
    elif link_style == LinkStyle("static"):
        if enable_profiling:
            return ctx.attrs.profiled_static_libs
        return ctx.attrs.static_libs
    elif link_style == LinkStyle("static_pic"):
        if enable_profiling:
            return ctx.attrs.pic_profiled_static_libs
        return ctx.attrs.pic_static_libs
    else:
        fail("Unexpected LinkStyle: " + link_style.value)

def haskell_prebuilt_library_impl(ctx: AnalysisContext) -> list[Provider]:
    # MergedLinkInfo for both with and without profiling
    native_infos = []
    prof_native_infos = []

    haskell_infos = []
    shared_library_infos = []
    for dep in ctx.attrs.deps:
        used = False
        if HaskellLinkInfo in dep:
            used = True
            haskell_infos.append(dep[HaskellLinkInfo])
        li = dep.get(MergedLinkInfo)
        if li != None:
            used = True
            native_infos.append(li)
            if HaskellLinkInfo not in dep:
                prof_native_infos.append(li)
        if HaskellProfLinkInfo in dep:
            prof_native_infos.append(dep[HaskellProfLinkInfo].prof_infos)
        if SharedLibraryInfo in dep:
            used = True
            shared_library_infos.append(dep[SharedLibraryInfo])
        if PythonLibraryInfo in dep:
            used = True
        if not used:
            fail("Unexpected link info encountered")

    hlibinfos = {}
    prof_hlibinfos = {}
    hlinkinfos = {}
    prof_hlinkinfos = {}
    link_infos = {}
    prof_link_infos = {}
    for link_style in LinkStyle:
        libs = _get_haskell_prebuilt_libs(ctx, link_style, False)
        prof_libs = _get_haskell_prebuilt_libs(ctx, link_style, True)

        hlibinfo = HaskellLibraryInfo(
            name = ctx.attrs.name,
            db = ctx.attrs.db,
            import_dirs = {},
            stub_dirs = [],
            id = ctx.attrs.id,
            libs = libs,
            version = ctx.attrs.version,
            is_prebuilt = True,
            profiling_enabled = False,
        )
        prof_hlibinfo = HaskellLibraryInfo(
            name = ctx.attrs.name,
            db = ctx.attrs.db,
            import_dirs = {},
            stub_dirs = [],
            id = ctx.attrs.id,
            libs = prof_libs,
            version = ctx.attrs.version,
            is_prebuilt = True,
            profiling_enabled = True,
        )

        def archive_linkable(lib):
            return ArchiveLinkable(
                archive = Archive(artifact = lib),
                linker_type = "gnu",
            )

        def shared_linkable(lib):
            return SharedLibLinkable(
                lib = lib,
            )

        linkables = [
            (shared_linkable if link_style == LinkStyle("shared") else archive_linkable)(lib)
            for lib in libs
        ]
        prof_linkables = [
            (shared_linkable if link_style == LinkStyle("shared") else archive_linkable)(lib)
            for lib in prof_libs
        ]

        hlibinfos[link_style] = hlibinfo
        hlinkinfos[link_style] = [hlibinfo]
        prof_hlibinfos[link_style] = prof_hlibinfo
        prof_hlinkinfos[link_style] = [prof_hlibinfo]
        link_infos[link_style] = LinkInfos(
            default = LinkInfo(
                pre_flags = ctx.attrs.exported_linker_flags,
                linkables = linkables,
            ),
        )
        prof_link_infos[link_style] = LinkInfos(
            default = LinkInfo(
                pre_flags = ctx.attrs.exported_linker_flags,
                linkables = prof_linkables,
            ),
        )

    haskell_link_infos = HaskellLinkInfo(
        info = hlinkinfos,
        prof_info = prof_hlinkinfos,
    )
    haskell_lib_provider = HaskellLibraryProvider(
        lib = hlibinfos,
        prof_lib = prof_hlibinfos,
    )

    # The link info that will be used when this library is a dependency of a non-Haskell
    # target (e.g. a cxx_library()). We need to pick the profiling libs if we're in
    # profiling mode.
    default_link_infos = prof_link_infos if ctx.attrs.enable_profiling else link_infos
    default_native_infos = prof_native_infos if ctx.attrs.enable_profiling else native_infos
    merged_link_info = create_merged_link_info(
        ctx,
        # We don't have access to a CxxToolchain here (yet).
        # Give that it's already built, this doesn't mean much, use a sane default.
        pic_behavior = PicBehavior("supported"),
        link_infos = {_to_lib_output_style(s): v for s, v in default_link_infos.items()},
        exported_deps = default_native_infos,
    )

    prof_merged_link_info = create_merged_link_info(
        ctx,
        # We don't have access to a CxxToolchain here (yet).
        # Give that it's already built, this doesn't mean much, use a sane default.
        pic_behavior = PicBehavior("supported"),
        link_infos = {_to_lib_output_style(s): v for s, v in prof_link_infos.items()},
        exported_deps = prof_native_infos,
    )

    solibs = {}
    for soname, lib in ctx.attrs.shared_libs.items():
        solibs[soname] = LinkedObject(output = lib, unstripped_output = lib)

    linkable_graph = create_linkable_graph(
        ctx,
        node = create_linkable_graph_node(
            ctx,
            linkable_node = create_linkable_node(
                ctx = ctx,
                exported_deps = ctx.attrs.deps,
                link_infos = {_to_lib_output_style(s): v for s, v in link_infos.items()},
                shared_libs = solibs,
                default_soname = None,
            ),
        ),
        deps = ctx.attrs.deps,
    )

    inherited_pp_info = cxx_inherited_preprocessor_infos(ctx.attrs.deps)
    own_pp_info = CPreprocessor(
        relative_args = CPreprocessorArgs(args = flatten([["-isystem", d] for d in ctx.attrs.cxx_header_dirs])),
    )

    return [
        DefaultInfo(),
        haskell_lib_provider,
        cxx_merge_cpreprocessors(ctx, [own_pp_info], inherited_pp_info),
        merge_shared_libraries(
            ctx.actions,
            create_shared_libraries(ctx, solibs),
            shared_library_infos,
        ),
        merge_link_group_lib_info(deps = ctx.attrs.deps),
        merge_haskell_link_infos(haskell_infos + [haskell_link_infos]),
        merged_link_info,
        HaskellProfLinkInfo(
            prof_infos = prof_merged_link_info,
        ),
        linkable_graph,
    ]

def merge_haskell_link_infos(deps: list[HaskellLinkInfo]) -> HaskellLinkInfo:
    merged = {}
    prof_merged = {}
    for link_style in LinkStyle:
        children = []
        prof_children = []
        for dep in deps:
            if link_style in dep.info:
                children.extend(dep.info[link_style])

            if link_style in dep.prof_info:
                prof_children.extend(dep.prof_info[link_style])

        merged[link_style] = dedupe(children)
        prof_merged[link_style] = dedupe(prof_children)

    return HaskellLinkInfo(info = merged, prof_info = prof_merged)

PackagesInfo = record(
    exposed_package_args = cmd_args,
    packagedb_args = cmd_args,
    transitive_deps = field(list[HaskellLibraryInfo]),
)

def _package_flag(toolchain: HaskellToolchainInfo) -> str:
    if toolchain.support_expose_package:
        return "-expose-package"
    else:
        return "-package"

def get_packages_info(
        ctx: AnalysisContext,
        link_style: LinkStyle,
        specify_pkg_version: bool,
        enable_profiling: bool) -> PackagesInfo:
    haskell_toolchain = ctx.attrs._haskell_toolchain[HaskellToolchainInfo]

    # Collect library dependencies. Note that these don't need to be in a
    # particular order and we really want to remove duplicates (there
    # are a *lot* of duplicates).
    libs = {}
    direct_deps_link_info = _attr_deps_haskell_link_infos(ctx)
    merged_hs_link_info = merge_haskell_link_infos(direct_deps_link_info)

    hs_link_info = merged_hs_link_info.prof_info if enable_profiling else merged_hs_link_info.info

    for lib in hs_link_info[link_style]:
        libs[lib.db] = lib  # lib.db is a good enough unique key

    # base is special and gets exposed by default
    package_flag = _package_flag(haskell_toolchain)
    exposed_package_args = cmd_args([package_flag, "base"])

    packagedb_args = cmd_args()

    for lib in libs.values():
        exposed_package_args.hidden(lib.import_dirs.values())
        exposed_package_args.hidden(lib.stub_dirs)

        # libs of dependencies might be needed at compile time if
        # we're using Template Haskell:
        exposed_package_args.hidden(lib.libs)

        packagedb_args.hidden(lib.import_dirs.values())
        packagedb_args.hidden(lib.stub_dirs)
        packagedb_args.hidden(lib.libs)

    for lib in libs.values():
        # These we need to add for all the packages/dependencies, i.e.
        # direct and transitive (e.g. `fbcode-common-hs-util-hs-array`)
        packagedb_args.add("-package-db", lib.db)

    haskell_direct_deps_lib_infos = _attr_deps_haskell_lib_infos(
        ctx,
        link_style,
        enable_profiling,
    )

    # Expose only the packages we depend on directly
    for lib in haskell_direct_deps_lib_infos:
        pkg_name = lib.name
        if (specify_pkg_version):
            pkg_name += "-{}".format(lib.version)

        exposed_package_args.add(package_flag, pkg_name)

    return PackagesInfo(
        exposed_package_args = exposed_package_args,
        packagedb_args = packagedb_args,
        transitive_deps = libs.values(),
    )

# The type of the return value of the `_compile()` function.
CompileResultInfo = record(
    objects = field(Artifact),
    hi = field(Artifact),
    stubs = field(Artifact),
    producing_indices = field(bool),
)

def _link_style_extensions(link_style: LinkStyle) -> (str, str):
    if link_style == LinkStyle("shared"):
        return ("dyn_o", "dyn_hi")
    elif link_style == LinkStyle("static_pic"):
        return ("o", "hi")  # is this right?
    elif link_style == LinkStyle("static"):
        return ("o", "hi")
    fail("unknown LinkStyle")

def _output_extensions(
        link_style: LinkStyle,
        profiled: bool) -> (str, str):
    osuf, hisuf = _link_style_extensions(link_style)
    if profiled:
        return ("p_" + osuf, "p_" + hisuf)
    else:
        return (osuf, hisuf)

def _srcs_to_objfiles(
        ctx: AnalysisContext,
        odir: Artifact,
        osuf: str) -> cmd_args:
    objfiles = cmd_args()
    for src, _ in _srcs_to_pairs(ctx.attrs.srcs):
        # Don't link boot sources, as they're only meant to be used for compiling.
        if _is_haskell_src(src):
            objfiles.add(cmd_args([odir, "/", paths.replace_extension(src, "." + osuf)], delimiter = ""))
    return objfiles

# We take a named_set for srcs, which is sometimes a list, sometimes a dict.
# In future we should only accept a list, but for now, cope with both.
def _srcs_to_pairs(srcs) -> list[(str, Artifact)]:
    if type(srcs) == type({}):
        return srcs.items()
    else:
        return [(src.short_path, src) for src in srcs]

# Single place to build the suffix used in artifacts (e.g. package directories,
# lib names) considering attributes like link style and profiling.
def get_artifact_suffix(link_style: LinkStyle, enable_profiling: bool) -> str:
    artifact_suffix = link_style.value
    if enable_profiling:
        artifact_suffix += "-prof"
    return artifact_suffix

# Compile all the context's sources.
def _compile(
        ctx: AnalysisContext,
        link_style: LinkStyle,
        enable_profiling: bool,
        extra_args = []) -> CompileResultInfo:
    haskell_toolchain = ctx.attrs._haskell_toolchain[HaskellToolchainInfo]
    compile_cmd = cmd_args(haskell_toolchain.compiler)
    compile_cmd.add(haskell_toolchain.compiler_flags)

    # Some rules pass in RTS (e.g. `+RTS ... -RTS`) options for GHC, which can't
    # be parsed when inside an argsfile.
    compile_cmd.add(ctx.attrs.compiler_flags)

    compile_args = cmd_args()
    compile_args.add("-no-link", "-i")

    if enable_profiling:
        compile_args.add("-prof")

    if link_style == LinkStyle("shared"):
        compile_args.add("-dynamic", "-fPIC")
    elif link_style == LinkStyle("static_pic"):
        compile_args.add("-fPIC", "-fexternal-dynamic-refs")

    osuf, hisuf = _output_extensions(link_style, enable_profiling)
    compile_args.add("-osuf", osuf, "-hisuf", hisuf)

    if getattr(ctx.attrs, "main", None) != None:
        compile_args.add(["-main-is", ctx.attrs.main])

    artifact_suffix = get_artifact_suffix(link_style, enable_profiling)

    objects = ctx.actions.declare_output(
        "objects-" + artifact_suffix,
        dir = True,
    )
    hi = ctx.actions.declare_output("hi-" + artifact_suffix, dir = True)
    stubs = ctx.actions.declare_output("stubs-" + artifact_suffix, dir = True)

    compile_args.add(
        "-odir",
        objects.as_output(),
        "-hidir",
        hi.as_output(),
        "-hiedir",
        hi.as_output(),
        "-stubdir",
        stubs.as_output(),
    )

    # Add -package-db and -package/-expose-package flags for each Haskell
    # library dependency.
    packages_info = get_packages_info(
        ctx,
        link_style,
        specify_pkg_version = False,
        enable_profiling = enable_profiling,
    )

    compile_args.add(packages_info.exposed_package_args)
    compile_args.add(packages_info.packagedb_args)

    # Add args from preprocess-able inputs.
    inherited_pre = cxx_inherited_preprocessor_infos(ctx.attrs.deps)
    pre = cxx_merge_cpreprocessors(ctx, [], inherited_pre)
    pre_args = pre.set.project_as_args("args")
    compile_args.add(cmd_args(pre_args, format = "-optP={}"))

    compile_args.add(extra_args)

    for (path, src) in _srcs_to_pairs(ctx.attrs.srcs):
        # hs-boot files aren't expected to be an argument to compiler but does need
        # to be included in the directory of the associated src file
        if _is_haskell_src(path):
            compile_args.add(src)
        else:
            compile_args.hidden(src)

    if haskell_toolchain.use_argsfile:
        argsfile = ctx.actions.declare_output(
            "haskell_compile_" + artifact_suffix + ".argsfile",
        )
        ctx.actions.write(argsfile.as_output(), compile_args, allow_args = True)
        compile_cmd.add(cmd_args(argsfile, format = "@{}").hidden(compile_args))
    else:
        compile_cmd.add(compile_args)

    ctx.actions.run(
        compile_cmd,
        category = "haskell_compile_" + artifact_suffix.replace("-", "_"),
        no_outputs_cleanup = True,
    )

    producing_indices = "-fwrite-ide-info" in ctx.attrs.compiler_flags

    return CompileResultInfo(
        objects = objects,
        hi = hi,
        stubs = stubs,
        producing_indices = producing_indices,
    )

_REGISTER_PACKAGE = """\
set -eu
GHC_PKG=$1
DB=$2
PKGCONF=$3
"$GHC_PKG" init "$DB"
"$GHC_PKG" register --package-conf "$DB" --no-expand-pkgroot "$PKGCONF"
"""

# Create a package
#
# The way we use packages is a bit strange. We're not using them
# at link time at all: all the linking info is in the
# HaskellLibraryInfo and we construct linker command lines
# manually. Packages are used for:
#
#  - finding .hi files at compile time
#
#  - symbol namespacing (so that modules with the same name in
#    different libraries don't clash).
#
#  - controlling module visibility: only dependencies that are
#    directly declared as dependencies may be used
#
#  - Template Haskell: the compiler needs to load libraries itself
#    at compile time, so it uses the package specs to find out
#    which libraries and where.
def _make_package(
        ctx: AnalysisContext,
        link_style: LinkStyle,
        pkgname: str,
        libname: str,
        hlis: list[HaskellLibraryInfo],
        hi: dict[bool, Artifact],
        lib: dict[bool, Artifact],
        enable_profiling: bool) -> Artifact:
    artifact_suffix = get_artifact_suffix(link_style, enable_profiling)

    # Don't expose boot sources, as they're only meant to be used for compiling.
    modules = [_src_to_module_name(x) for x, _ in _srcs_to_pairs(ctx.attrs.srcs) if _is_haskell_src(x)]

    uniq_hlis = {}
    for x in hlis:
        uniq_hlis[x.id] = x

    if enable_profiling:
        # Add the `-p` suffix otherwise ghc will look for objects
        # following this logic (https://fburl.com/code/3gmobm5x) and will fail.
        libname += "_p"

    def mk_artifact_dir(dir_prefix: str, profiled: bool) -> str:
        art_suff = get_artifact_suffix(link_style, profiled)
        return "\"${pkgroot}/" + dir_prefix + "-" + art_suff + "\""

    import_dirs = [
        mk_artifact_dir("hi", profiled)
        for profiled in hi.keys()
    ]
    library_dirs = [
        mk_artifact_dir("lib", profiled)
        for profiled in hi.keys()
    ]

    conf = [
        "name: " + pkgname,
        "version: 1.0.0",
        "id: " + pkgname,
        "key: " + pkgname,
        "exposed: False",
        "exposed-modules: " + ", ".join(modules),
        "import-dirs:" + ", ".join(import_dirs),
        "library-dirs:" + ", ".join(library_dirs),
        "extra-libraries: " + libname,
        "depends: " + ", ".join(uniq_hlis),
    ]
    pkg_conf = ctx.actions.write("pkg-" + artifact_suffix + ".conf", conf)

    db = ctx.actions.declare_output("db-" + artifact_suffix)

    db_deps = {}
    for x in uniq_hlis.values():
        db_deps[repr(x.db)] = x.db

    # So that ghc-pkg can find the DBs for the dependencies. We might
    # be able to use flags for this instead, but this works.
    ghc_package_path = cmd_args(
        db_deps.values(),
        delimiter = ":",
    )

    haskell_toolchain = ctx.attrs._haskell_toolchain[HaskellToolchainInfo]
    ctx.actions.run(
        cmd_args([
            "sh",
            "-c",
            _REGISTER_PACKAGE,
            "",
            haskell_toolchain.packager,
            db.as_output(),
            pkg_conf,
        ]).hidden(hi.values()).hidden(lib.values()),  # needs hi, because ghc-pkg checks that the .hi files exist
        category = "haskell_package_" + artifact_suffix.replace("-", "_"),
        env = {"GHC_PACKAGE_PATH": ghc_package_path},
    )

    return db

HaskellLibBuildOutput = record(
    hlib = HaskellLibraryInfo,
    solibs = dict[str, LinkedObject],
    link_infos = LinkInfos,
    compiled = CompileResultInfo,
    libs = list[Artifact],
)

def _build_haskell_lib(
        ctx,
        hlis: list[HaskellLinkInfo],  # haskell link infos from all deps
        nlis: list[MergedLinkInfo],  # native link infos from all deps
        link_style: LinkStyle,
        enable_profiling: bool,
        # The non-profiling artifacts are also needed to build the package for
        # profiling, so it should be passed when `enable_profiling` is True.
        non_profiling_hlib: [HaskellLibBuildOutput, None] = None) -> HaskellLibBuildOutput:
    linker_info = ctx.attrs._cxx_toolchain[CxxToolchainInfo].linker_info
    libname = repr(ctx.label.path).replace("//", "_").replace("/", "_") + "_" + ctx.label.name
    pkgname = libname.replace("_", "-")

    # Link the objects into a library
    haskell_toolchain = ctx.attrs._haskell_toolchain[HaskellToolchainInfo]

    osuf, _hisuf = _output_extensions(link_style, enable_profiling)

    # Compile the sources
    compiled = _compile(
        ctx,
        link_style,
        enable_profiling = enable_profiling,
        extra_args = ["-this-unit-id", pkgname],
    )
    solibs = {}
    artifact_suffix = get_artifact_suffix(link_style, enable_profiling)

    libstem = libname
    if link_style == LinkStyle("static_pic"):
        libstem += "_pic"

    static_lib_suffix = "_p.a" if enable_profiling else ".a"
    libfile = "lib" + libstem + (".so" if link_style == LinkStyle("shared") else static_lib_suffix)

    lib_short_path = paths.join("lib-{}".format(artifact_suffix), libfile)

    linfos = [x.prof_info if enable_profiling else x.info for x in hlis]
    uniq_infos = dedupe(flatten([x[link_style] for x in linfos]))

    objfiles = _srcs_to_objfiles(ctx, compiled.objects, osuf)

    if link_style == LinkStyle("shared"):
        lib = ctx.actions.declare_output(lib_short_path)
        link = cmd_args(haskell_toolchain.linker)
        link.add(haskell_toolchain.linker_flags)
        link.add(ctx.attrs.linker_flags)
        link.add("-o", lib.as_output())
        link.add(
            "-shared",
            "-dynamic",
            "-optl",
            "-Wl,-soname",
            "-optl",
            "-Wl," + libfile,
        )

        link.add(objfiles)
        link.hidden(compiled.stubs)

        infos = get_link_args_for_strategy(
            ctx,
            nlis,
            to_link_strategy(link_style),
        )
        link.add(cmd_args(unpack_link_args(infos), prepend = "-optl"))
        ctx.actions.run(
            link,
            category = "haskell_link" + artifact_suffix.replace("-", "_"),
        )

        solibs[libfile] = LinkedObject(output = lib, unstripped_output = lib)
        libs = [lib]
        link_infos = LinkInfos(
            default = LinkInfo(linkables = [SharedLibLinkable(lib = lib)]),
        )

    else:  # static flavours
        # TODO: avoid making an archive for a single object, like cxx does
        # (but would that work with Template Haskell?)
        archive = make_archive(ctx, lib_short_path, [compiled.objects], objfiles)
        lib = archive.artifact
        libs = [lib] + archive.external_objects
        link_infos = LinkInfos(
            default = LinkInfo(
                linkables = [
                    ArchiveLinkable(
                        archive = archive,
                        linker_type = linker_info.type,
                        link_whole = ctx.attrs.link_whole,
                    ),
                ],
            ),
        )

    if enable_profiling and link_style != LinkStyle("shared"):
        if not non_profiling_hlib:
            fail("Non-profiling HaskellLibBuildOutput wasn't provided when building profiling lib")

        import_artifacts = {
            True: compiled.hi,
            False: non_profiling_hlib.compiled.hi,
        }
        library_artifacts = {
            True: lib,
            False: non_profiling_hlib.libs[0],
        }
        all_libs = libs + non_profiling_hlib.libs
        stub_dirs = [compiled.stubs] + [non_profiling_hlib.compiled.stubs]
    else:
        import_artifacts = {
            False: compiled.hi,
        }
        library_artifacts = {
            False: lib,
        }
        all_libs = libs
        stub_dirs = [compiled.stubs]

    db = _make_package(
        ctx,
        link_style,
        pkgname,
        libstem,
        uniq_infos,
        import_artifacts,
        library_artifacts,
        enable_profiling = enable_profiling,
    )

    hlib = HaskellLibraryInfo(
        name = pkgname,
        db = db,
        id = pkgname,
        import_dirs = import_artifacts,
        stub_dirs = stub_dirs,
        libs = all_libs,
        version = "1.0.0",
        is_prebuilt = False,
        profiling_enabled = enable_profiling,
    )

    return HaskellLibBuildOutput(
        hlib = hlib,
        solibs = solibs,
        link_infos = link_infos,
        compiled = compiled,
        libs = libs,
    )

def haskell_library_impl(ctx: AnalysisContext) -> list[Provider]:
    preferred_linkage = _attr_preferred_linkage(ctx)
    if ctx.attrs.enable_profiling and preferred_linkage == Linkage("any"):
        preferred_linkage = Linkage("static")

    # Get haskell and native link infos from all deps
    hlis = []
    nlis = []
    prof_nlis = []
    shared_library_infos = []
    for lib in attr_deps(ctx):
        li = lib.get(HaskellLinkInfo)
        if li != None:
            hlis.append(li)
        li = lib.get(MergedLinkInfo)
        if li != None:
            nlis.append(li)
            if HaskellLinkInfo not in lib:
                # MergedLinkInfo from non-haskell deps should be part of the
                # profiling MergedLinkInfo
                prof_nlis.append(li)
        li = lib.get(HaskellProfLinkInfo)
        if li != None:
            prof_nlis.append(li.prof_infos)
        li = lib.get(SharedLibraryInfo)
        if li != None:
            shared_library_infos.append(li)

    solibs = {}
    link_infos = {}
    prof_link_infos = {}
    hlib_infos = {}
    hlink_infos = {}
    prof_hlib_infos = {}
    prof_hlink_infos = {}
    indexing_tsets = {}
    sub_targets = {}

    # The non-profiling library is also needed to build the package with
    # profiling enabled, so we need to keep track of it for each link style.
    non_profiling_hlib = {}
    for enable_profiling in [False, True]:
        for output_style in get_output_styles_for_linkage(preferred_linkage):
            link_style = legacy_output_style_to_link_style(output_style)
            if link_style == LinkStyle("shared") and enable_profiling:
                # Profiling isn't support with dynamic linking
                continue

            hlib_build_out = _build_haskell_lib(
                ctx,
                hlis = hlis,
                nlis = nlis,
                link_style = link_style,
                enable_profiling = enable_profiling,
                non_profiling_hlib = non_profiling_hlib.get(link_style),
            )
            if not enable_profiling:
                non_profiling_hlib[link_style] = hlib_build_out

            hlib = hlib_build_out.hlib
            solibs.update(hlib_build_out.solibs)
            compiled = hlib_build_out.compiled
            libs = hlib_build_out.libs

            if enable_profiling:
                prof_hlib_infos[link_style] = hlib
                prof_hlink_infos[link_style] = [hlib]
                prof_link_infos[link_style] = hlib_build_out.link_infos
            else:
                hlib_infos[link_style] = hlib
                hlink_infos[link_style] = [hlib]
                link_infos[link_style] = hlib_build_out.link_infos

            # Build the indices and create subtargets only once, with profiling
            # enabled or disabled based on what was set in the library's
            # target.
            if ctx.attrs.enable_profiling == enable_profiling:
                if compiled.producing_indices:
                    tset = derive_indexing_tset(
                        ctx.actions,
                        link_style,
                        compiled.hi,
                        attr_deps(ctx),
                    )
                    indexing_tsets[link_style] = tset

                sub_targets[link_style.value.replace("_", "-")] = [DefaultInfo(
                    default_outputs = libs,
                )]

    pic_behavior = ctx.attrs._cxx_toolchain[CxxToolchainInfo].pic_behavior
    link_style = _cxx_toolchain_link_style(ctx)
    output_style = get_lib_output_style(
        to_link_strategy(link_style),
        preferred_linkage,
        pic_behavior,
    )

    # TODO(cjhopman): this haskell implementation does not consistently handle LibOutputStyle
    # and LinkStrategy as expected and it's hard to tell what the intent of the existing code is
    # and so we currently just preserve its existing use of the legacy LinkStyle type and just
    # naively convert it at the boundaries of other code. This needs to be cleaned up by someone
    # who understands the intent of the code here.
    actual_link_style = legacy_output_style_to_link_style(output_style)

    if preferred_linkage != Linkage("static"):
        # Profiling isn't support with dynamic linking, but `prof_link_infos`
        # needs entries for all link styles.
        # We only need to set the shared link_style in both `prof_link_infos`
        # and `link_infos` if the target doesn't force static linking.
        prof_link_infos[LinkStyle("shared")] = link_infos[LinkStyle("shared")]

    default_link_infos = prof_link_infos if ctx.attrs.enable_profiling else link_infos
    default_native_infos = prof_nlis if ctx.attrs.enable_profiling else nlis
    merged_link_info = create_merged_link_info(
        ctx,
        pic_behavior = pic_behavior,
        link_infos = {_to_lib_output_style(s): v for s, v in default_link_infos.items()},
        preferred_linkage = preferred_linkage,
        exported_deps = default_native_infos,
    )

    prof_merged_link_info = create_merged_link_info(
        ctx,
        pic_behavior = pic_behavior,
        link_infos = {_to_lib_output_style(s): v for s, v in prof_link_infos.items()},
        preferred_linkage = preferred_linkage,
        exported_deps = prof_nlis,
    )

    linkable_graph = create_linkable_graph(
        ctx,
        node = create_linkable_graph_node(
            ctx,
            linkable_node = create_linkable_node(
                ctx = ctx,
                preferred_linkage = preferred_linkage,
                exported_deps = ctx.attrs.deps,
                link_infos = {_to_lib_output_style(s): v for s, v in link_infos.items()},
                shared_libs = solibs,
                # TODO(cjhopman): this should be set to non-None
                default_soname = None,
            ),
        ),
        deps = ctx.attrs.deps,
    )

    default_output = hlib_infos[actual_link_style].libs

    inherited_pp_info = cxx_inherited_preprocessor_infos(attr_deps(ctx))

    # We would like to expose the generated _stub.h headers to C++
    # compilations, but it's hard to do that without overbuilding. Which
    # link_style should we pick below? If we pick a different link_style from
    # the one being used by the root rule, we'll end up building all the
    # Haskell libraries multiple times.
    #
    #    pp = [CPreprocessor(
    #        args =
    #            flatten([["-isystem", dir] for dir in hlib_infos[actual_link_style].stub_dirs]),
    #    )]
    pp = []

    providers = [
        DefaultInfo(
            default_outputs = default_output,
            sub_targets = sub_targets,
        ),
        HaskellLibraryProvider(
            lib = hlib_infos,
            prof_lib = prof_hlib_infos,
        ),
        merge_haskell_link_infos(hlis + [HaskellLinkInfo(
            info = hlink_infos,
            prof_info = prof_hlink_infos,
        )]),
        merged_link_info,
        HaskellProfLinkInfo(
            prof_infos = prof_merged_link_info,
        ),
        linkable_graph,
        cxx_merge_cpreprocessors(ctx, pp, inherited_pp_info),
        merge_shared_libraries(
            ctx.actions,
            create_shared_libraries(ctx, solibs),
            shared_library_infos,
        ),
    ]

    if indexing_tsets:
        providers.append(HaskellIndexInfo(info = indexing_tsets))

    # TODO(cjhopman): This code is for templ_vars is duplicated from cxx_library
    templ_vars = {}

    # Add in ldflag macros.
    for link_style in (LinkStyle("static"), LinkStyle("static_pic")):
        name = "ldflags-" + link_style.value.replace("_", "-")
        args = cmd_args()
        linker_info = ctx.attrs._cxx_toolchain[CxxToolchainInfo].linker_info
        args.add(linker_info.linker_flags)
        args.add(unpack_link_args(
            get_link_args_for_strategy(
                ctx,
                [merged_link_info],
                to_link_strategy(link_style),
            ),
        ))
        templ_vars[name] = args

    # TODO(T110378127): To implement `$(ldflags-shared ...)` properly, we'd need
    # to setup a symink tree rule for all transitive shared libs.  Since this
    # currently would be pretty costly (O(N^2)?), and since it's not that
    # commonly used anyway, just use `static-pic` instead.  Longer-term, once
    # v1 is gone, macros that use `$(ldflags-shared ...)` (e.g. Haskell's
    # hsc2hs) can move to a v2 rules-based API to avoid needing this macro.
    templ_vars["ldflags-shared"] = templ_vars["ldflags-static-pic"]

    providers.append(TemplatePlaceholderInfo(keyed_variables = templ_vars))

    providers.append(merge_link_group_lib_info(deps = attr_deps(ctx)))

    return providers

# TODO(cjhopman): should this be LibOutputType or LinkStrategy?
def derive_indexing_tset(
        actions: AnalysisActions,
        link_style: LinkStyle,
        value: [Artifact, None],
        children: list[Dependency]) -> HaskellIndexingTSet:
    index_children = []
    for dep in children:
        li = dep.get(HaskellIndexInfo)
        if li:
            if (link_style in li.info):
                index_children.append(li.info[link_style])

    return actions.tset(
        HaskellIndexingTSet,
        value = value,
        children = index_children,
    )

def haskell_binary_impl(ctx: AnalysisContext) -> list[Provider]:
    enable_profiling = ctx.attrs.enable_profiling

    # Decide what kind of linking we're doing

    link_style = _attr_link_style(ctx)

    # Link Groups
    link_group_info = get_link_group_info(ctx, filter_and_map_idx(LinkableGraph, attr_deps(ctx)))

    # Profiling doesn't support shared libraries
    if enable_profiling and link_style == LinkStyle("shared"):
        link_style = LinkStyle("static")

    compiled = _compile(
        ctx,
        link_style,
        enable_profiling = enable_profiling,
    )

    haskell_toolchain = ctx.attrs._haskell_toolchain[HaskellToolchainInfo]

    output = ctx.actions.declare_output(ctx.attrs.name)
    link = cmd_args(haskell_toolchain.compiler)
    link.add("-o", output.as_output())
    link.add(haskell_toolchain.linker_flags)
    link.add(ctx.attrs.linker_flags)

    link.hidden(compiled.stubs)

    osuf, _hisuf = _output_extensions(link_style, enable_profiling)

    objfiles = _srcs_to_objfiles(ctx, compiled.objects, osuf)
    link.add(objfiles)

    indexing_tsets = {}
    if compiled.producing_indices:
        tset = derive_indexing_tset(ctx.actions, link_style, compiled.hi, attr_deps(ctx))
        indexing_tsets[link_style] = tset

    slis = []
    for lib in attr_deps(ctx):
        li = lib.get(SharedLibraryInfo)
        if li != None:
            slis.append(li)
    shlib_info = merge_shared_libraries(
        ctx.actions,
        deps = slis,
    )

    sos = {}

    if link_group_info != None:
        own_binary_link_flags = []
        auto_link_groups = {}
        link_group_libs = {}
        link_deps = linkables(attr_deps(ctx))
        linkable_graph_node_map = get_linkable_graph_node_map_func(link_group_info.graph)()
        link_group_preferred_linkage = get_link_group_preferred_linkage(link_group_info.groups.values())

        # If we're using auto-link-groups, where we generate the link group links
        # in the prelude, the link group map will give us the link group libs.
        # Otherwise, pull them from the `LinkGroupLibInfo` provider from out deps.
        auto_link_group_specs = get_auto_link_group_specs(ctx, link_group_info)
        if auto_link_group_specs != None:
            linked_link_groups = create_link_groups(
                ctx = ctx,
                link_group_mappings = link_group_info.mappings,
                link_group_preferred_linkage = link_group_preferred_linkage,
                executable_deps = [d.linkable_graph.nodes.value.label for d in link_deps if d.linkable_graph != None],
                link_group_specs = auto_link_group_specs,
                linkable_graph_node_map = linkable_graph_node_map,
            )
            for name, linked_link_group in linked_link_groups.libs.items():
                auto_link_groups[name] = linked_link_group.artifact
                if linked_link_group.library != None:
                    link_group_libs[name] = linked_link_group.library
            own_binary_link_flags += linked_link_groups.symbol_ldflags

        else:
            # NOTE(agallagher): We don't use version scripts and linker scripts
            # for non-auto-link-group flow, as it's note clear it's useful (e.g.
            # it's mainly for supporting dlopen-enabled libs and extensions).
            link_group_libs = gather_link_group_libs(
                children = [d.link_group_lib_info for d in link_deps],
            )

        link_group_relevant_roots = find_relevant_roots(
            linkable_graph_node_map = linkable_graph_node_map,
            link_group_mappings = link_group_info.mappings,
            roots = [
                mapping.root
                for group in link_group_info.groups.values()
                for mapping in group.mappings
                if mapping.root != None
            ],
        )

        labels_to_links_map = get_filtered_labels_to_links_map(
            linkable_graph_node_map = linkable_graph_node_map,
            link_group = None,
            link_groups = link_group_info.groups,
            link_group_mappings = link_group_info.mappings,
            link_group_preferred_linkage = link_group_preferred_linkage,
            link_group_libs = {
                name: (lib.label, lib.shared_link_infos)
                for name, lib in link_group_libs.items()
            },
            link_strategy = to_link_strategy(link_style),
            roots = (
                [
                    d.linkable_graph.nodes.value.label
                    for d in link_deps
                    if d.linkable_graph != None
                ] +
                link_group_relevant_roots
            ),
            is_executable_link = True,
            force_static_follows_dependents = True,
            pic_behavior = PicBehavior("supported"),
        )

        # NOTE: Our Haskell DLL support impl currently links transitive haskell
        # deps needed by DLLs which get linked into the main executable as link-
        # whole.  To emulate this, we mark Haskell rules with a special label
        # and traverse this to find all the nodes we need to link whole.
        public_nodes = []
        if ctx.attrs.link_group_public_deps_label != None:
            public_nodes = get_transitive_deps_matching_labels(
                linkable_graph_node_map = linkable_graph_node_map,
                label = ctx.attrs.link_group_public_deps_label,
                roots = link_group_relevant_roots,
            )

        link_infos = []
        link_infos.append(
            LinkInfo(
                pre_flags = own_binary_link_flags,
            ),
        )
        link_infos.extend(get_filtered_links(labels_to_links_map, set(public_nodes)))
        infos = LinkArgs(infos = link_infos)

        link_group_ctx = LinkGroupContext(
            link_group_mappings = link_group_info.mappings,
            link_group_libs = link_group_libs,
            link_group_preferred_linkage = link_group_preferred_linkage,
            labels_to_links_map = labels_to_links_map,
        )

        for name, shared_lib in traverse_shared_library_info(shlib_info).items():
            label = shared_lib.label
            if is_link_group_shlib(label, link_group_ctx):
                sos[name] = shared_lib.lib

        # When there are no matches for a pattern based link group,
        # `link_group_mappings` will not have an entry associated with the lib.
        for _name, link_group_lib in link_group_libs.items():
            sos.update(link_group_lib.shared_libs)

    else:
        nlis = []
        for lib in attr_deps(ctx):
            if enable_profiling:
                hli = lib.get(HaskellProfLinkInfo)
                if hli != None:
                    nlis.append(hli.prof_infos)
                    continue
            li = lib.get(MergedLinkInfo)
            if li != None:
                nlis.append(li)
        for name, shared_lib in traverse_shared_library_info(shlib_info).items():
            sos[name] = shared_lib.lib
        infos = get_link_args_for_strategy(ctx, nlis, to_link_strategy(link_style))

    link.add(cmd_args(unpack_link_args(infos), prepend = "-optl"))

    ctx.actions.run(link, category = "haskell_link")

    run = cmd_args(output)

    if link_style == LinkStyle("shared") or link_group_info != None:
        sos_dir = "__{}__shared_libs_symlink_tree".format(ctx.attrs.name)
        link.add("-optl", "-Wl,-rpath", "-optl", "-Wl,$ORIGIN/{}".format(sos_dir))
        symlink_dir = ctx.actions.symlinked_dir(sos_dir, {n: o.output for n, o in sos.items()})
        run.hidden(symlink_dir)

    providers = [
        DefaultInfo(default_output = output),
        RunInfo(args = run),
    ]

    if indexing_tsets:
        providers.append(HaskellIndexInfo(info = indexing_tsets))

    return providers
