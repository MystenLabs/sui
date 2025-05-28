# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(
    "@prelude//:artifact_tset.bzl",
    "ArtifactTSet",
    "make_artifact_tset",
)
load("@prelude//cxx:cxx_toolchain_types.bzl", "PicBehavior")
load(
    "@prelude//cxx:linker.bzl",
    "get_link_whole_args",
    "get_no_as_needed_shared_libs_flags",
    "get_objects_as_library_args",
)
load("@prelude//utils:arglike.bzl", "ArgLike")
load(
    "@prelude//utils:utils.bzl",
    "flatten",
)

# Represents an archive (.a file)
Archive = record(
    artifact = field(Artifact),
    # For a thin archive, this contains all the referenced .o files
    external_objects = field(list[Artifact], []),
)

# The different strategies that are used to determine sharedlib/executable
# link constituents.
# These default link strategies will each traverse dependenciess and collect
# all transitive dependencies that will link as archives until it reaches ones that will
# link as shared libs. For each dependency, this link strategy and that dependency's
# preferred_linkage will determine which LibOutputStyle will be used.
LinkStrategy = enum(
    # Prefers that dependencies be included in the link line as archives of native objects
    "static",
    # Prefers that dependencies be included in the link line as archives of PIC native objects
    "static_pic",
    # Prefers that dependencies be included in the link line as shared libraries
    "shared",
)

# A legacy type that was previously used to encode both LibOutputStyle and LinkStrategy. Uses should
# be updated to use the appropriate new enum.
LinkStyle = enum(
    "static",
    "static_pic",
    "shared",
)

# We still read link_style (and some related) attrs to LinkStyle but then usually convert them
# immediately to LinkStrategy.
# TODO(cjhopman): We should migrate the attr name to `link_strategy`
def to_link_strategy(link_style: LinkStyle) -> LinkStrategy:
    return LinkStrategy(link_style.value)

# The different types of outputs of native library. Which specific output style to use for a library
# will depend on the link_strategy that is being computed and the library's preferred_linkage.
LibOutputStyle = enum(
    "archive",
    "pic_archive",
    "shared_lib",
)

def default_output_style_for_link_strategy(link_strategy: LinkStrategy) -> LibOutputStyle:
    if link_strategy == LinkStrategy("static"):
        return LibOutputStyle("archive")
    if link_strategy == LinkStrategy("static_pic"):
        return LibOutputStyle("pic_archive")
    return LibOutputStyle("shared_lib")

# Ways a library can request to be linked (e.g. usually specific via a rule
# param like `preferred_linkage`.  The actual link style used for a library is
# usually determined by a combination of this and the link style being exported
# via a provider.
Linkage = enum(
    "static",
    "shared",
    "any",
)

# An archive.
ArchiveLinkable = record(
    # Artifact in the .a format from ar
    archive = field(Archive),
    # If a bitcode bundle was created for this artifact it will be present here
    bitcode_bundle = field([Artifact, None], None),
    linker_type = field(str),
    link_whole = field(bool, False),
    # Indicates if this archive may contain LTO bit code.  Can be set to `False`
    # to e.g. tell dist LTO handling that a potentially expensive archive doesn't
    # need to be processed.
    supports_lto = field(bool, True),
)

# A shared lib.
SharedLibLinkable = record(
    lib = field(Artifact),
    link_without_soname = field(bool, False),
)

# A list of objects.
ObjectsLinkable = record(
    objects = field([list[Artifact], None], None),
    # Any of the objects that are in bitcode format
    bitcode_bundle = field([Artifact, None], None),
    linker_type = field(str),
    link_whole = field(bool, False),
)

# Framework + library information for Apple/Cxx targets.
FrameworksLinkable = record(
    # A list of trimmed framework paths, example: ["Foundation", "UIKit"]
    # Used to construct `-framework` args.
    framework_names = field(list[str], []),
    # A list of unresolved framework paths (i.e., containing $SDKROOT, etc).
    # Used to construct `-F` args for compilation and linking.
    #
    # Framework path resolution _must_ happen at the target site because
    # different targets might use different toolchains. For example,
    # an `apple_library()` might get _compiled_ using one toolchain
    # and then linked by as part of an `apple_binary()` using another
    # compatible toolchain. The resolved framework directories passed
    # using `-F` would be different for the compilation and the linking.
    unresolved_framework_paths = field(list[str], []),
    # A list of library names, used to construct `-l` args.
    library_names = field(list[str], []),
)

SwiftmoduleLinkable = record(
    swiftmodules = field(ArtifactTSet, ArtifactTSet()),
)

# Represents the Swift runtime as a linker input.
SwiftRuntimeLinkable = record(
    # Only store whether the runtime is required, so that linker flags
    # are only materialized _once_ (no duplicates) on the link line.
    runtime_required = field(bool, False),
)

LinkableTypes = [ArchiveLinkable, SharedLibLinkable, ObjectsLinkable, FrameworksLinkable, SwiftRuntimeLinkable, SwiftmoduleLinkable]

LinkerFlags = record(
    flags = field(list[typing.Any], []),
    post_flags = field(list[typing.Any], []),
    exported_flags = field(list[typing.Any], []),
    exported_post_flags = field(list[typing.Any], []),
)

# Contains the information required to add an item (often corresponding to a single library) to a link command line.
LinkInfo = record(
    # An informative name for this LinkInfo. This may be used in user messages
    # or when constructing intermediate output paths and does not need to be unique.
    name = field([str, None], None),
    # Opaque cmd_arg-likes to be added pre/post this item on a linker command line.
    pre_flags = field(list[typing.Any], []),
    post_flags = field(list[typing.Any], []),
    # Primary input to the linker, one of the Linkable types above.
    linkables = field(list[LinkableTypes], []),
    # Debug info which is referenced -- but not included -- by linkables in the
    # link info.  For example, this may include `.dwo` files, or the original
    # `.o` files if they contain debug info that doesn't follow the link.
    external_debug_info = field(ArtifactTSet, ArtifactTSet()),
)

# The ordering to use when traversing linker libs transitive sets.
LinkOrdering = enum(
    # Preorder traversal, the default behavior which traverses depth-first returning the current
    # node, and then its children left-to-right.
    "preorder",
    # Topological sort, such that nodes are listed after all nodes that have them as descendants.
    "topological",
)

def set_link_info_link_whole(info: LinkInfo) -> LinkInfo:
    linkables = [set_linkable_link_whole(linkable) for linkable in info.linkables]
    return LinkInfo(
        name = info.name,
        pre_flags = info.pre_flags,
        post_flags = info.post_flags,
        linkables = linkables,
        external_debug_info = info.external_debug_info,
    )

def set_linkable_link_whole(
        linkable: [ArchiveLinkable, ObjectsLinkable, SharedLibLinkable, FrameworksLinkable]) -> [ArchiveLinkable, ObjectsLinkable, SharedLibLinkable, FrameworksLinkable]:
    if isinstance(linkable, ArchiveLinkable):
        return ArchiveLinkable(
            archive = linkable.archive,
            linker_type = linkable.linker_type,
            link_whole = True,
            supports_lto = linkable.supports_lto,
        )
    elif isinstance(linkable, ObjectsLinkable):
        return ObjectsLinkable(
            objects = linkable.objects,
            linker_type = linkable.linker_type,
            link_whole = True,
        )
    return linkable

# Helper to wrap a LinkInfo with additional pre/post-flags.
def wrap_link_info(
        inner: LinkInfo,
        pre_flags: list[typing.Any] = [],
        post_flags: list[typing.Any] = []) -> LinkInfo:
    pre_flags = pre_flags + inner.pre_flags
    post_flags = inner.post_flags + post_flags
    return LinkInfo(
        name = inner.name,
        pre_flags = pre_flags,
        post_flags = post_flags,
        linkables = inner.linkables,
        external_debug_info = inner.external_debug_info,
    )

# Adds appropriate args representing `linkable` to `args`
def append_linkable_args(args: cmd_args, linkable: LinkableTypes):
    if isinstance(linkable, ArchiveLinkable):
        if linkable.link_whole:
            args.add(get_link_whole_args(linkable.linker_type, [linkable.archive.artifact]))
        elif linkable.linker_type == "darwin":
            pass
        else:
            args.add(linkable.archive.artifact)

        # When using thin archives, object files are implicitly used as inputs
        # to the link, so make sure track them as inputs so that they're
        # materialized/tracked properly.
        args.add(cmd_args().hidden(linkable.archive.external_objects))
    elif isinstance(linkable, SharedLibLinkable):
        if linkable.link_without_soname:
            args.add(cmd_args(linkable.lib, format = "-L{}").parent())
            args.add("-l" + linkable.lib.basename.removeprefix("lib").removesuffix(linkable.lib.extension))
        else:
            args.add(linkable.lib)
    elif isinstance(linkable, ObjectsLinkable):
        # We depend on just the filelist for darwin linker and don't add the normal args
        if linkable.linker_type != "darwin":
            # We need to export every symbol when link groups are used, but enabling
            # --whole-archive with --start-lib is undefined behavior in gnu linkers:
            # https://reviews.llvm.org/D120443. We need to export symbols from every
            # linkable in the link_info
            if not linkable.link_whole:
                args.add(get_objects_as_library_args(linkable.linker_type, linkable.objects))
            else:
                args.add(linkable.objects)
    elif isinstance(linkable, FrameworksLinkable) or isinstance(linkable, SwiftRuntimeLinkable) or isinstance(linkable, SwiftmoduleLinkable):
        # These flags are handled separately so they can be deduped.
        #
        # We've seen in apps with larger dependency graphs that failing
        # to dedupe these args results in linker.argsfile which are too big.
        pass
    else:
        fail("Encountered unhandled linkable {}".format(str(linkable)))

def link_info_to_args(value: LinkInfo) -> cmd_args:
    args = cmd_args(value.pre_flags)
    for linkable in value.linkables:
        append_linkable_args(args, linkable)
    if False:
        # TODO(nga): `post_flags` is never `None`.
        def unknown():
            pass

        value = unknown()
    if value.post_flags != None:
        args.add(value.post_flags)
    return args

# List of inputs to pass to the darwin linker via the `-filelist` param.
# TODO(agallagher): It might be nicer to leave these inlined in the args
# above and extract them at link time via reflection.  This way we'd hide
# platform-specific details from this level.
# NOTE(agallagher): Using filelist out-of-band means objects/archives get
# linked out of order of their corresponding flags.
def link_info_filelist(value: LinkInfo) -> list[Artifact]:
    filelists = []
    for linkable in value.linkables:
        if isinstance(linkable, ArchiveLinkable):
            if linkable.linker_type == "darwin" and not linkable.link_whole:
                filelists.append(linkable.archive.artifact)
        elif isinstance(linkable, SharedLibLinkable):
            pass
        elif isinstance(linkable, ObjectsLinkable):
            if linkable.linker_type == "darwin":
                filelists += linkable.objects
        elif isinstance(linkable, FrameworksLinkable) or isinstance(linkable, SwiftRuntimeLinkable) or isinstance(linkable, SwiftmoduleLinkable):
            pass
        else:
            fail("Encountered unhandled linkable {}".format(str(linkable)))
    return filelists

# Encapsulate all `LinkInfo`s provided by a given rule's link style.
#
# We provide both the "default" and (optionally) a pre-"stripped" LinkInfo. For a consumer that doesn't care
# about debug info (for example, who is going to produce stripped output anyway), it can be significantly
# cheaper to consume the pre-stripped LinkInfo.
LinkInfos = record(
    # Link info to use by default.
    default = field(LinkInfo),
    # Link info stripped of debug symbols.
    stripped = field([LinkInfo, None], None),
)

def _link_info_default_args(infos: LinkInfos):
    info = infos.default
    return link_info_to_args(info)

def _link_info_default_shared_link_args(infos: LinkInfos):
    info = infos.default
    return link_info_to_args(info)

def _link_info_stripped_args(infos: LinkInfos):
    info = infos.stripped or infos.default
    return link_info_to_args(info)

def _link_info_stripped_shared_link_args(infos: LinkInfos):
    info = infos.stripped or infos.default
    return link_info_to_args(info)

def _link_info_default_filelist(infos: LinkInfos):
    info = infos.default
    return link_info_filelist(info)

def _link_info_stripped_filelist(infos: LinkInfos):
    info = infos.stripped or infos.default
    return link_info_filelist(info)

def _link_info_has_default_filelist(children: list[bool], infos: [LinkInfos, None]) -> bool:
    if infos:
        info = infos.default
        if link_info_filelist(info):
            return True
    return any(children)

def _link_info_has_stripped_filelist(children: list[bool], infos: [LinkInfos, None]) -> bool:
    if infos:
        info = infos.stripped or infos.default
        if link_info_filelist(info):
            return True
    return any(children)

# TransitiveSet of LinkInfos.
LinkInfosTSet = transitive_set(
    args_projections = {
        "default": _link_info_default_args,
        "default_filelist": _link_info_default_filelist,
        "default_shared": _link_info_default_shared_link_args,
        "stripped": _link_info_stripped_args,
        "stripped_filelist": _link_info_stripped_filelist,
        "stripped_shared": _link_info_stripped_shared_link_args,
    },
    reductions = {
        "has_default_filelist": _link_info_has_default_filelist,
        "has_stripped_filelist": _link_info_has_stripped_filelist,
    },
)

LinkArgsTSet = record(
    infos = field(LinkInfosTSet),
    external_debug_info = field(ArtifactTSet, ArtifactTSet()),
    prefer_stripped = field(bool, False),
)

# An enum. Only one field should be set. The variants here represent different
# ways in which we might obtain linker commands: through a t-set of propagated
# dependencies (used for deps propagated unconditionally up a tree), through a
# series of LinkInfo (used for link groups, Omnibus linking), or simply through
# raw arguments we want to include (used for e.g. per-target link flags).
LinkArgs = record(
    # A LinkInfosTSet + a flag indicating if stripped is preferred.
    tset = field([LinkArgsTSet, None], None),
    # A list of LinkInfos
    infos = field([list[LinkInfo], None], None),
    # A bunch of flags.
    flags = field([ArgLike, None], None),
)

# The output of a native link (e.g. a shared library or an executable).
LinkedObject = record(
    output = field([Artifact, Promise]),
    # The combined bitcode from this linked object and any static libraries
    bitcode_bundle = field([Artifact, None], None),
    # the generated linked output before running stripping(and bolt).
    unstripped_output = field(Artifact),
    # the generated linked output before running bolt, may be None if bolt is not used.
    prebolt_output = field([Artifact, None], None),
    # The LinkArgs used to produce this LinkedObject. This can be useful for debugging or
    # for downstream rules to reproduce the shared library with some modifications (for example
    # android relinker will link again with an added version script argument).
    link_args = field([LinkArgs, None], None),
    # A linked object (binary/shared library) may have an associated dwp file with
    # its corresponding DWARF debug info.
    # May be None when Split DWARF is disabled or for some types of synthetic link objects.
    dwp = field([Artifact, None], None),
    # Additional dirs or paths that contain debug info referenced by the linked
    # object (e.g. split dwarf files or PDB file).
    external_debug_info = field(ArtifactTSet, ArtifactTSet()),
    # This argsfile is generated in the `cxx_link` step and contains a list of arguments
    # passed to the linker. It is being exposed as a sub-target for debugging purposes.
    linker_argsfile = field([Artifact, None], None),
    # The filelist is generated in the `cxx_link` step and contains a list of
    # object files (static libs or plain object files) passed to the linker.
    # It is being exposed for debugging purposes. Only present when a Darwin
    # linker is used.
    linker_filelist = field([Artifact, None], None),
    # The linker command as generated by `cxx_link`. Exposed for debugging purposes only.
    # Not present for DistLTO scenarios.
    linker_command = field([cmd_args, None], None),
    # This sub-target is only available for distributed thinLTO builds.
    index_argsfile = field([Artifact, None], None),
    # Import library for linking with DLL on Windows.
    # If not on Windows it's always None.
    import_library = field([Artifact, None], None),
    # A linked object (binary/shared library) may have an associated PDB file with
    # its corresponding Windows debug info.
    # If not on Windows it's always None.
    pdb = field([Artifact, None], None),
    # Split-debug info generated by the link.
    split_debug_output = field([Artifact, None], None),
)

# A map of native linkable infos from transitive dependencies for each LinkStrategy.
# This contains the information about how to link in a target for each link strategy.
# This doesn't contain the information about things needed to package the linked result
# (i.e. this doesn't contain the information needed to know what shared libs needed at runtime
# for the final result).
MergedLinkInfo = provider(fields = [
    "_infos",  # dict[LinkStrategy, LinkInfosTSet]
    "_external_debug_info",  # dict[LinkStrategy, ArtifactTSet]
    # Apple framework linker args must be deduped to avoid overflow in our argsfiles.
    #
    # To save on repeated computation of transitive LinkInfos, we store a dedupped
    # structure, based on the link-style.
    "frameworks",  # dict[LinkStrategy, FrameworksLinkable | None]
    "swiftmodules",  # dict[LinkStrategy, SwiftmoduleLinkable | None]
    "swift_runtime",  # dict[LinkStrategy, SwiftRuntimeLinkable | None]
])

# A map of linkages to all possible output styles it supports.
_LIB_OUTPUT_STYLES_FOR_LINKAGE = {
    Linkage("any"): [LibOutputStyle("archive"), LibOutputStyle("pic_archive"), LibOutputStyle("shared_lib")],
    Linkage("static"): [LibOutputStyle("archive"), LibOutputStyle("pic_archive")],
    Linkage("shared"): [LibOutputStyle("pic_archive"), LibOutputStyle("shared_lib")],
}

# Helper to wrap a LinkInfos with additional pre/post-flags.
def wrap_link_infos(
        inner: LinkInfos,
        pre_flags: list[typing.Any] = [],
        post_flags: list[typing.Any] = []) -> LinkInfos:
    return LinkInfos(
        default = wrap_link_info(
            inner.default,
            pre_flags = pre_flags,
            post_flags = post_flags,
        ),
        stripped = None if inner.stripped == None else wrap_link_info(
            inner.stripped,
            pre_flags = pre_flags,
            post_flags = post_flags,
        ),
    )

def create_merged_link_info(
        # Target context for which to create the link info.
        ctx: AnalysisContext,
        pic_behavior: PicBehavior,
        # The outputs available for this rule, as a map from LibOutputStyle (as
        # used by dependents) to `LinkInfo`.
        link_infos: dict[LibOutputStyle, LinkInfos] = {},
        # How the rule requests to be linked.  This will be used to determine
        # which actual link style to propagate for each "requested" link style.
        preferred_linkage: Linkage = Linkage("any"),
        # Link info to propagate from non-exported deps for static link styles.
        deps: list[MergedLinkInfo] = [],
        # Link info to always propagate from exported deps.
        exported_deps: list[MergedLinkInfo] = [],
        frameworks_linkable: [FrameworksLinkable, None] = None,
        swiftmodule_linkable: [SwiftmoduleLinkable, None] = None,
        swift_runtime_linkable: [SwiftRuntimeLinkable, None] = None) -> MergedLinkInfo:
    """
    Create a `MergedLinkInfo` provider.
    """

    infos = {}
    external_debug_info = {}
    frameworks = {}
    swift_runtime = {}
    swiftmodules = {}

    # We don't know how this target will be linked, so we generate the possible
    # link info given the target's preferred linkage, to be consumed by the
    # ultimate linking target.
    for link_strategy in LinkStrategy:
        actual_output_style = get_lib_output_style(link_strategy, preferred_linkage, pic_behavior)

        children = []
        external_debug_info_children = []
        framework_linkables = []
        swift_runtime_linkables = []
        swiftmodule_linkables = []

        # When we're being linked statically, we also need to export all private
        # linkable input (e.g. so that any unresolved symbols we have are
        # resolved properly when we're linked).
        if actual_output_style != LibOutputStyle("shared_lib"):
            # We never want to propagate the linkables used to build a shared library.
            #
            # Doing so breaks the encapsulation of what is in linked in the library vs. the main executable.
            framework_linkables.append(frameworks_linkable)
            framework_linkables += [dep_info.frameworks[link_strategy] for dep_info in exported_deps]

            swiftmodule_linkables.append(swiftmodule_linkable)
            swiftmodule_linkables += [dep_info.swiftmodules[link_strategy] for dep_info in exported_deps]

            swift_runtime_linkables.append(swift_runtime_linkable)
            swift_runtime_linkables += [dep_info.swift_runtime[link_strategy] for dep_info in exported_deps]

            for dep_info in deps:
                children.append(dep_info._infos[link_strategy])
                external_debug_info_children.append(dep_info._external_debug_info[link_strategy])
                framework_linkables.append(dep_info.frameworks[link_strategy])
                swiftmodule_linkables.append(dep_info.swiftmodules[link_strategy])
                swift_runtime_linkables.append(dep_info.swift_runtime[link_strategy])

        # We always export link info for exported deps.
        for dep_info in exported_deps:
            children.append(dep_info._infos[link_strategy])
            external_debug_info_children.append(dep_info._external_debug_info[link_strategy])

        frameworks[link_strategy] = merge_framework_linkables(framework_linkables)
        swift_runtime[link_strategy] = merge_swift_runtime_linkables(swift_runtime_linkables)
        swiftmodules[link_strategy] = merge_swiftmodule_linkables(ctx, swiftmodule_linkables)

        if actual_output_style in link_infos:
            link_info = link_infos[actual_output_style]

            # TODO(cjhopman): This seems like we won't propagate information about our children unless this target itself
            # has an output for this strategy. Why is that correct?
            infos[link_strategy] = ctx.actions.tset(
                LinkInfosTSet,
                value = link_info,
                children = children,
            )
            external_debug_info[link_strategy] = make_artifact_tset(
                actions = ctx.actions,
                label = ctx.label,
                children = (
                    [link_info.default.external_debug_info] +
                    external_debug_info_children
                ),
            )

    return MergedLinkInfo(
        _infos = infos,
        _external_debug_info = external_debug_info,
        frameworks = frameworks,
        swift_runtime = swift_runtime,
        swiftmodules = swiftmodules,
    )

def create_merged_link_info_for_propagation(
        ctx: AnalysisContext,
        xs: list[MergedLinkInfo]) -> MergedLinkInfo:
    """
    Creates a MergedLinkInfo for a node that just propagates up its dependencies' MergedLinkInfo without contributing anything itself.

    A node that contributes something itself would use create_merged_link_info.
    """
    merged = {}
    merged_external_debug_info = {}
    frameworks = {}
    swift_runtime = {}
    swiftmodules = {}
    for link_strategy in LinkStrategy:
        merged[link_strategy] = ctx.actions.tset(
            LinkInfosTSet,
            children = filter(None, [x._infos.get(link_strategy) for x in xs]),
        )
        merged_external_debug_info[link_strategy] = make_artifact_tset(
            actions = ctx.actions,
            label = ctx.label,
            children = filter(None, [x._external_debug_info.get(link_strategy) for x in xs]),
        )
        frameworks[link_strategy] = merge_framework_linkables([x.frameworks[link_strategy] for x in xs])
        swift_runtime[link_strategy] = merge_swift_runtime_linkables([x.swift_runtime[link_strategy] for x in xs])
        swiftmodules[link_strategy] = merge_swiftmodule_linkables(ctx, [x.swiftmodules[link_strategy] for x in xs])
    return MergedLinkInfo(
        _infos = merged,
        _external_debug_info = merged_external_debug_info,
        frameworks = frameworks,
        swift_runtime = swift_runtime,
        swiftmodules = swiftmodules,
    )

def get_link_info(
        infos: LinkInfos,
        prefer_stripped: bool = False) -> LinkInfo:
    """
    Helper for getting a `LinkInfo` out of a `LinkInfos`.
    """

    # When requested, prefer using pre-stripped link info.
    if prefer_stripped and infos.stripped != None:
        return infos.stripped

    return infos.default

def unpack_link_args(args: LinkArgs, is_shared: [bool, None] = None, link_ordering: [LinkOrdering, None] = None) -> ArgLike:
    if args.tset != None:
        ordering = link_ordering.value if link_ordering else "preorder"

        tset = args.tset.infos
        if is_shared:
            if args.tset.prefer_stripped:
                return tset.project_as_args("stripped_shared", ordering = ordering)
            return tset.project_as_args("default_shared", ordering = ordering)
        else:
            if args.tset.prefer_stripped:
                return tset.project_as_args("stripped", ordering = ordering)
            return tset.project_as_args("default", ordering = ordering)

    if args.infos != None:
        return cmd_args([link_info_to_args(info) for info in args.infos])

    if args.flags != None:
        return args.flags

    fail("Unpacked invalid empty link args")

def unpack_link_args_filelist(args: LinkArgs) -> [ArgLike, None]:
    if args.tset != None:
        tset = args.tset.infos
        stripped = args.tset.prefer_stripped
        if not tset.reduce("has_stripped_filelist" if stripped else "has_default_filelist"):
            return None
        return tset.project_as_args("stripped_filelist" if stripped else "default_filelist")

    if args.infos != None:
        filelist = flatten([link_info_filelist(info) for info in args.infos])
        if not filelist:
            return None

        # Actually create cmd_args so the API is consistent between the 2 branches.
        args = cmd_args()
        args.add(filelist)
        return args

    if args.flags != None:
        return None

    fail("Unpacked invalid empty link args")

def unpack_external_debug_info(actions: AnalysisActions, args: LinkArgs) -> ArtifactTSet:
    if args.tset != None:
        if args.tset.prefer_stripped:
            return ArtifactTSet()
        return args.tset.external_debug_info

    if args.infos != None:
        return make_artifact_tset(
            actions = actions,
            children = [info.external_debug_info for info in args.infos],
        )

    if args.flags != None:
        return ArtifactTSet()

    fail("Unpacked invalid empty link args")

def map_to_link_infos(links: list[LinkArgs]) -> list[LinkInfo]:
    res = []

    def append(v):
        if v.pre_flags or v.post_flags or v.linkables:
            res.append(v)

    for link in links:
        if link.tset != None:
            for info in link.tset.infos.traverse():
                if link.tset.prefer_stripped:
                    append(info.stripped or info.default)
                else:
                    append(info.default)
            continue
        if link.infos != None:
            for link in link.infos:
                append(link)
            continue
        if link.flags != None:
            append(LinkInfo(pre_flags = link.flags))
            continue
        fail("Unpacked invalid empty link args")
    return res

def get_link_args_for_strategy(
        ctx: AnalysisContext,
        deps_merged_link_infos: list[MergedLinkInfo],
        link_strategy: LinkStrategy,
        prefer_stripped: bool = False,
        additional_link_info: [LinkInfo, None] = None) -> LinkArgs:
    """
    Derive the `LinkArgs` for a strategy and strip preference from a list of dependency's MergedLinkInfo.
    """

    infos_kwargs = {}
    if additional_link_info:
        infos_kwargs = {"value": LinkInfos(default = additional_link_info, stripped = additional_link_info)}
    infos = ctx.actions.tset(
        LinkInfosTSet,
        children = filter(None, [x._infos.get(link_strategy) for x in deps_merged_link_infos]),
        **infos_kwargs
    )
    external_debug_info = make_artifact_tset(
        actions = ctx.actions,
        label = ctx.label,
        children = filter(
            None,
            [x._external_debug_info.get(link_strategy) for x in deps_merged_link_infos] + ([additional_link_info.external_debug_info] if additional_link_info else []),
        ),
    )

    return LinkArgs(
        tset = LinkArgsTSet(
            infos = infos,
            external_debug_info = external_debug_info,
            prefer_stripped = prefer_stripped,
        ),
    )

def get_lib_output_style(
        requested_link_strategy: LinkStrategy,
        preferred_linkage: Linkage,
        pic_behavior: PicBehavior) -> LibOutputStyle:
    """
    Return what lib output style to use for a library for a requested link style and preferred linkage.
    --------------------------------------------------------------
    | preferred_linkage |              link_strategy             |
    |                   |----------------------------------------|
    |                   | static   | static_pic   |   shared     |
    -------------------------------------------------------------|
    |      static       | *archive | *pic_archive | *pic_archive |
    |      shared       | shared   |   shared     |   shared     |
    |       any         | *archive | *pic_archive |   shared     |
    --------------------------------------------------------------

    Either of *static or *static_pic may be changed to the other based on the pic_behavior
    """
    no_pic_style = _get_lib_output_style_without_pic_behavior(requested_link_strategy, preferred_linkage)
    return process_output_style_for_pic_behavior(no_pic_style, pic_behavior)

def _get_lib_output_style_without_pic_behavior(requested_link_strategy: LinkStrategy, preferred_linkage: Linkage) -> LibOutputStyle:
    if preferred_linkage == Linkage("any"):
        return default_output_style_for_link_strategy(requested_link_strategy)
    elif preferred_linkage == Linkage("shared"):
        return LibOutputStyle("shared_lib")
    else:  # preferred_linkage = static
        if requested_link_strategy == LinkStrategy("static"):
            return LibOutputStyle("archive")
        else:
            return LibOutputStyle("pic_archive")

def process_link_strategy_for_pic_behavior(link_strategy: LinkStrategy, behavior: PicBehavior) -> LinkStrategy:
    """
    Converts static/static_pic link styles to the appropriate static form according to the pic behavior.
    """
    if link_strategy == LinkStrategy("shared"):
        return link_strategy
    elif behavior == PicBehavior("supported"):
        return link_strategy
    elif behavior == PicBehavior("not_supported"):
        return LinkStrategy("static")
    elif behavior == PicBehavior("always_enabled"):
        return LinkStrategy("static_pic")
    else:
        fail("Unknown pic_behavior: {}".format(behavior))

# TODO(cjhopman): I think we should be able to make it an error to request an output style that
# violates the PicBehavior if we consistently translate LinkStyle and other top-level requests (i.e.
# it seems like the need to translate output styles points to us missing a translation at some higher level).
def process_output_style_for_pic_behavior(output_style: LibOutputStyle, behavior: PicBehavior) -> LibOutputStyle:
    """
    Converts archive/archive_pic output styles to the appropriate output form according to the pic behavior.
    """
    if output_style == LibOutputStyle("shared_lib"):
        return output_style
    elif behavior == PicBehavior("supported"):
        return output_style
    elif behavior == PicBehavior("not_supported"):
        return LibOutputStyle("archive")
    elif behavior == PicBehavior("always_enabled"):
        return LibOutputStyle("pic_archive")
    else:
        fail("Unknown pic_behavior: {}".format(behavior))

def subtarget_for_output_style(output_style: LibOutputStyle) -> str:
    # TODO(cjhopman): This preserves historical strings for these (when we used LinkStyle for both link strategies and
    # output styles). It would be good to update that to match the LibOutputStyle.
    return legacy_output_style_to_link_style(output_style).value.replace("_", "-")

def get_output_styles_for_linkage(linkage: Linkage) -> list[LibOutputStyle]:
    """
    Return all possible `LibOutputStyle`s that apply for the given `Linkage`.
    """
    return _LIB_OUTPUT_STYLES_FOR_LINKAGE[linkage]

def legacy_output_style_to_link_style(output_style: LibOutputStyle) -> LinkStyle:
    """
    We previously used LinkStyle to represent both the type of a library output and for the different default link strategies.

    To support splitting those two concepts, some places are still using LinkStyle when they probably should be using LibOutputStyle.
    """
    if output_style == LibOutputStyle("shared_lib"):
        return LinkStyle("shared")
    elif output_style == LibOutputStyle("archive"):
        return LinkStyle("static")
    elif output_style == LibOutputStyle("pic_archive"):
        return LinkStyle("static_pic")
    fail("unrecognized output_style {}".format(output_style))

def merge_swift_runtime_linkables(linkables: list[[SwiftRuntimeLinkable, None]]) -> SwiftRuntimeLinkable:
    for linkable in linkables:
        if linkable and linkable.runtime_required:
            return SwiftRuntimeLinkable(runtime_required = True)
    return SwiftRuntimeLinkable(runtime_required = False)

def merge_framework_linkables(linkables: list[[FrameworksLinkable, None]]) -> FrameworksLinkable:
    unique_framework_names = {}
    unique_framework_paths = {}
    unique_library_names = {}
    for linkable in linkables:
        if not linkable:
            continue

        # Avoid building a huge list and then de-duplicating, instead we
        # use a set to track each used entry, order does not matter.
        for framework in linkable.framework_names:
            unique_framework_names[framework] = True
        for framework_path in linkable.unresolved_framework_paths:
            unique_framework_paths[framework_path] = True
        for library_name in linkable.library_names:
            unique_library_names[library_name] = True

    return FrameworksLinkable(
        framework_names = unique_framework_names.keys(),
        unresolved_framework_paths = unique_framework_paths.keys(),
        library_names = unique_library_names.keys(),
    )

def merge_swiftmodule_linkables(ctx: AnalysisContext, linkables: list[[SwiftmoduleLinkable, None]]) -> SwiftmoduleLinkable:
    return SwiftmoduleLinkable(swiftmodules = make_artifact_tset(
        actions = ctx.actions,
        label = ctx.label,
        children = [
            linkable.swiftmodules
            for linkable in linkables
            if linkable != None
        ],
    ))

def wrap_with_no_as_needed_shared_libs_flags(linker_type: str, link_info: LinkInfo) -> LinkInfo:
    """
    Wrap link info in args used to prevent linkers from dropping unused shared
    library dependencies from the e.g. DT_NEEDED tags of the link.
    """

    if linker_type == "gnu":
        return wrap_link_info(
            inner = link_info,
            pre_flags = (
                ["-Wl,--push-state"] +
                get_no_as_needed_shared_libs_flags(linker_type)
            ),
            post_flags = ["-Wl,--pop-state"],
        )

    if linker_type == "darwin":
        return link_info

    fail("Linker type {} not supported".format(linker_type))

# Represents information to debug linker commands. Used to carry information
# about link commands.
LinkCommandDebugOutput = record(
    # The filename of the linkable output.
    filename = str,
    command = ArgLike,
    argsfile = Artifact,
    filelist = [Artifact, None],
)

# NB: Debug output is _not_ transitive over deps, so tsets are not used here.
LinkCommandDebugOutputInfo = provider(
    fields = [
        "debug_outputs",  # ["LinkCommandDebugOutput"]
    ],
)

UnstrippedLinkOutputInfo = provider(fields = {
    "artifact": Artifact,
})

def make_link_command_debug_output(linked_object: LinkedObject) -> [LinkCommandDebugOutput, None]:
    if not linked_object.output or not linked_object.linker_command or not linked_object.linker_argsfile:
        return None
    return LinkCommandDebugOutput(
        filename = linked_object.output.short_path,
        command = linked_object.linker_command,
        argsfile = linked_object.linker_argsfile,
        filelist = linked_object.linker_filelist,
    )

# Given a list of `LinkCommandDebugOutput`, it will produce a JSON info file.
# The JSON info file will contain entries for each link command. In addition,
# it will _not_ materialize any inputs to the link command except:
# - linker argfile
# - linker filelist (if present - only applicable to Darwin linkers)
def make_link_command_debug_output_json_info(ctx: AnalysisContext, debug_outputs: list[LinkCommandDebugOutput]) -> Artifact:
    json_info = []
    associated_artifacts = []
    for debug_output in debug_outputs:
        json_info.append({
            "command": debug_output.command,
            "filename": debug_output.filename,
        })

        # Ensure all argsfile and filelists get materialized, as those are needed for debugging
        associated_artifacts.extend(filter(None, [debug_output.argsfile, debug_output.filelist]))

    # Explicitly drop all inputs by using `with_inputs = False`, we don't want
    # to materialize all inputs to the link actions (which includes all object files
    # and possibly other shared libraries).
    json_output = ctx.actions.write_json("linker.command", json_info, with_inputs = False)
    json_output_with_artifacts = json_output.with_associated_artifacts(associated_artifacts)
    return json_output_with_artifacts
