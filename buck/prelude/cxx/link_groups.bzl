# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//:paths.bzl", "paths")
load("@prelude//linking:execution_preference.bzl", "LinkExecutionPreference")
load(
    "@prelude//linking:link_groups.bzl",
    "LinkGroupLib",
)
load(
    "@prelude//linking:link_info.bzl",
    "LibOutputStyle",
    "LinkArgs",
    "LinkInfo",
    "LinkInfos",
    "LinkStrategy",
    "Linkage",
    "LinkedObject",  # @unused Used as a type
    "SharedLibLinkable",
    "get_lib_output_style",
    "set_link_info_link_whole",
    "wrap_link_info",
    "wrap_with_no_as_needed_shared_libs_flags",
    get_link_info_from_link_infos = "get_link_info",
)
load(
    "@prelude//linking:linkable_graph.bzl",
    "LinkableGraph",  # @unused Used as a type
    "LinkableNode",  # @unused Used as a type
    "LinkableRootInfo",  # @unused Used as a type
    "create_linkable_graph",
    "get_deps_for_link",
    "get_link_info",
    "get_linkable_graph_node_map_func",
    "get_transitive_deps",
)
load("@prelude//utils:arglike.bzl", "ArgLike")
load(
    "@prelude//utils:graph_utils.bzl",
    "breadth_first_traversal_by",
)
load(
    "@prelude//utils:set.bzl",
    "set",
    "set_record",
)
load(
    "@prelude//utils:utils.bzl",
    "expect",
    "value_or",
)
load(":cxx_context.bzl", "get_cxx_toolchain_info")
load(
    ":cxx_library_utility.bzl",
    "cxx_is_gnu",
)
load(":cxx_toolchain_types.bzl", "PicBehavior")
load(
    ":groups.bzl",
    "Group",  # @unused Used as a type
    "MATCH_ALL_LABEL",
    "NO_MATCH_LABEL",
    "compute_mappings",
    "parse_groups_definitions",
)
load(
    ":link.bzl",
    "cxx_link_shared_library",
)
load(
    ":link_types.bzl",
    "link_options",
)
load(
    ":linker.bzl",
    "get_ignore_undefined_symbols_flags",
)
load(
    ":shared_library_interface.bzl",
    "shared_library_interface",
)
load(
    ":symbols.bzl",
    "create_dynamic_list_version_script",
    "extract_global_syms",
    "extract_undefined_syms",
    "get_undefined_symbols_args",
)

# TODO(cjhopman): This deserves some high-level docs about the link group linking strategy.

# TODO(cjhopman): I think it would be good to document/explain why link group linking
# strategy also uses one of the default LinkStrategy.

# Returns a list of targets that belong in the current link group. If using
# auto_link_groups, this sub_target will only be available to use with the
# main binary, and it will return a list of all targets that map to a link
# group.
LINK_GROUP_MAP_DATABASE_SUB_TARGET = "link-group-map-database"
LINK_GROUP_MAP_DATABASE_FILENAME = "link_group_map_database.json"

# Returns a mapping from link group names to its constituent targets.
LINK_GROUP_MAPPINGS_SUB_TARGET = "link-group-mappings"
LINK_GROUP_MAPPINGS_FILENAME_SUFFIX = ".link_group_map.json"

LinkGroupInfo = provider(
    # @unsorted-dict-items
    fields = {
        "groups": provider_field(typing.Any, default = None),  # dict[str, Group]
        "groups_hash": provider_field(typing.Any, default = None),  # str
        "mappings": provider_field(typing.Any, default = None),  # dict[ConfiguredProvidersLabel, str]
        # Additional graphs needed to cover labels referenced by the groups above.
        # This is useful in cases where the consumer of this provider won't already
        # have deps covering these.
        # NOTE(agallagher): We do this to maintain existing behavior w/ the
        # standalone `link_group_map()` rule, but it's not clear if it's actually
        # desirable behavior.
        "graph": provider_field(typing.Any, default = None),  # LinkableGraph
    },
)

LinkGroupLinkInfo = record(
    link_info = field(LinkInfo),
    output_style = field(LibOutputStyle),
)

LinkGroupLibSpec = record(
    # The output name given to the linked shared object.
    name = field(str),
    # Used to differentiate normal native shared libs from e.g. Python native
    # extensions (which are technically shared libs, but don't set a SONAME
    # and aren't managed by `SharedLibraryInfo`s).
    is_shared_lib = field(bool, True),
    # Optional linkable root info that should be used to "guide" the link of
    # this link group.  This is useful for linking e.g. standalone shared libs
    # which may require special private linker flags (like version scripts) to
    # link.
    root = field([LinkableRootInfo, None], None),
    # The link group to link.
    group = field(Group),
)

_LinkedLinkGroup = record(
    artifact = field(LinkedObject),
    library = field([LinkGroupLib, None], None),
)

_LinkedLinkGroups = record(
    libs = field(dict[str, _LinkedLinkGroup]),
    symbol_ldflags = field(list[typing.Any], []),
)

def get_link_group(ctx: AnalysisContext) -> [str, None]:
    return ctx.attrs.link_group

def build_link_group_info(
        graph: LinkableGraph,
        groups: list[Group],
        min_node_count: [int, None] = None) -> LinkGroupInfo:
    linkable_graph_node_map = get_linkable_graph_node_map_func(graph)()

    # Filter out groups which don't meet the node count requirement.
    filtered_groups = {}
    node_count = value_or(min_node_count, len(linkable_graph_node_map))
    for group in groups:
        if group.attrs.enable_if_node_count_exceeds != None and node_count < group.attrs.enable_if_node_count_exceeds:
            continue
        filtered_groups[group.name] = group

    mappings = compute_mappings(
        groups = filtered_groups.values(),
        graph_map = linkable_graph_node_map,
    )

    return LinkGroupInfo(
        groups = filtered_groups,
        groups_hash = hash(str(filtered_groups)),
        mappings = mappings,
        # The consumer of this info may not have deps that cover that labels
        # referenced in our roots, so propagate graphs for them.
        # NOTE(agallagher): We do this to maintain existing behavior here
        # but it's not clear if it's actually desirable behavior.
        graph = graph,
    )

def get_link_group_info(
        ctx: AnalysisContext,
        executable_deps: [list[LinkableGraph], None] = None) -> [LinkGroupInfo, None]:
    """
    Parses the currently analyzed context for any link group definitions
    and returns a list of all link groups with their mappings.
    """
    link_group_map = ctx.attrs.link_group_map

    if not link_group_map:
        return None

    # If specified as a dep that provides the `LinkGroupInfo`, use that.
    if isinstance(link_group_map, Dependency):
        return link_group_map[LinkGroupInfo]

    # Otherwise build one from our graph.
    expect(executable_deps != None)
    link_groups = parse_groups_definitions(link_group_map)
    linkable_graph = create_linkable_graph(
        ctx,
        deps = (
            executable_deps +
            [d[LinkableGraph] for d in getattr(ctx.attrs, "link_group_deps", [])]
        ),
    )
    return build_link_group_info(
        graph = linkable_graph,
        groups = link_groups,
        min_node_count = getattr(ctx.attrs, "link_group_min_binary_node_count", 0),
    )

def get_link_group_preferred_linkage(link_groups: list[Group]) -> dict[Label, Linkage]:
    return {
        mapping.root: mapping.preferred_linkage
        for group in link_groups
        for mapping in group.mappings
        if mapping.root != None and mapping.preferred_linkage != None
    }

LinkGroupContext = record(
    link_group_mappings = field([dict[Label, str], None]),
    # TODO(nga): before, `Any` was `_LinkedLinkGroup`,
    #   which was treated as `Any`, because name starts with underscore.
    #   Put proper type here.
    link_group_libs = field(dict[str, typing.Any]),
    link_group_preferred_linkage = field(dict[Label, Linkage]),
    labels_to_links_map = field(dict[Label, LinkGroupLinkInfo]),
)

def is_link_group_shlib(
        label: Label,
        ctx: LinkGroupContext):
    # If this maps to a link group which we have a `LinkGroupLibInfo` for,
    # then we'll handle this outside of this function
    if label in ctx.link_group_mappings and ctx.link_group_mappings[label] in ctx.link_group_libs:
        return False

    # buildifier: disable=uninitialized
    if ctx.link_group_preferred_linkage.get(label, Linkage("any")) == Linkage("shared"):
        return True

    # if using link_groups, only materialize the link_group shlibs
    # buildifier: disable=uninitialized
    node_link = ctx.labels_to_links_map.get(label)
    if node_link != None and node_link.output_style == LibOutputStyle("shared_lib"):
        return True

    return False

def _transitively_update_shared_linkage(
        linkable_graph_node_map: dict[Label, LinkableNode],
        link_group: [str, None],
        link_strategy: LinkStrategy,
        link_group_preferred_linkage: dict[Label, Linkage],
        link_group_roots: dict[Label, str],
        pic_behavior: PicBehavior):
    # Identify targets whose shared linkage style may be propagated to
    # dependencies. Implicitly created root libraries are skipped.
    shared_lib_roots = []
    for target in link_group_preferred_linkage:
        # This preferred-linkage mapping may not actually be in the link graph.
        node = linkable_graph_node_map.get(target)
        if node == None:
            continue
        output_style = get_lib_output_style(link_strategy, link_group_preferred_linkage.get(target, node.preferred_linkage), pic_behavior)
        if output_style == LibOutputStyle("shared_lib"):
            target_link_group = link_group_roots.get(target)
            if target_link_group == None or target_link_group == link_group:
                shared_lib_roots.append(target)

    # buildifier: disable=uninitialized
    def process_dependency(node: Label) -> list[Label]:
        linkable_node = linkable_graph_node_map[node]
        if linkable_node.preferred_linkage == Linkage("any"):
            link_group_preferred_linkage[node] = Linkage("shared")
        return get_deps_for_link(linkable_node, link_strategy, pic_behavior)

    breadth_first_traversal_by(
        linkable_graph_node_map,
        shared_lib_roots,
        process_dependency,
    )

def get_filtered_labels_to_links_map(
        linkable_graph_node_map: dict[Label, LinkableNode],
        link_group: [str, None],
        link_groups: dict[str, Group],
        link_group_mappings: [dict[Label, str], None],
        link_group_preferred_linkage: dict[Label, Linkage],
        link_strategy: LinkStrategy,
        roots: list[Label],
        pic_behavior: PicBehavior,
        link_group_libs: dict[str, ([Label, None], LinkInfos)] = {},
        prefer_stripped: bool = False,
        is_executable_link: bool = False,
        force_static_follows_dependents: bool = True) -> dict[Label, LinkGroupLinkInfo]:
    """
    Given a linkable graph, link style and link group mappings, finds all links
    to consider for linking traversing the graph as necessary and then
    identifies which link infos and targets belong the in the provided link group.
    If no link group is provided, all unmatched link infos are returned.
    """

    def get_potential_linkables(node: Label) -> list[Label]:
        linkable_node = linkable_graph_node_map[node]  # buildifier: disable=uninitialized

        # Always link against exported deps
        node_linkables = list(linkable_node.exported_deps)

        # If the preferred linkage is `static` or `any` we need to link against the deps too.
        # TODO(cjhopman): This code originally was as commented out below and the comment indicated that the
        # intent was to not traverse in the second case if link style was shared, but at this point idk which
        # behavior we actually want.
        should_traverse_private_deps = False
        if linkable_node.preferred_linkage == Linkage("static"):
            should_traverse_private_deps = True
        elif linkable_node.preferred_linkage == Linkage("any"):
            should_traverse_private_deps = True
            # should_traverse = link_style != Linkage("shared")

        if should_traverse_private_deps:
            node_linkables += linkable_node.deps

        return node_linkables

    # Get all potential linkable targets
    linkables = breadth_first_traversal_by(
        linkable_graph_node_map,
        roots,
        get_potential_linkables,
    )

    # An index of target to link group names, for all link group library nodes.
    # Provides fast lookup of a link group root lib via it's label.
    link_group_roots = {
        label: name
        for name, (label, _) in link_group_libs.items()
        if label != None
    }

    # Transitively update preferred linkage to avoid runtime issues from
    # missing dependencies (e.g. for prebuilt shared libs).
    _transitively_update_shared_linkage(
        linkable_graph_node_map,
        link_group,
        link_strategy,
        link_group_preferred_linkage,
        link_group_roots,
        pic_behavior,
    )

    linkable_map = {}

    # Keep track of whether we've already added a link group to the link line
    # already.  This avoids use adding the same link group lib multiple times,
    # for each of the possible multiple nodes that maps to it.
    link_group_added = {}

    def add_link(target: Label, output_style: LibOutputStyle):
        linkable_map[target] = LinkGroupLinkInfo(
            link_info = get_link_info(linkable_graph_node_map[target], output_style, prefer_stripped),
            output_style = output_style,
        )  # buildifier: disable=uninitialized

    def add_link_group(target: Label, target_group: str):
        # If we've already added this link group to the link line, we're done.
        if target_group in link_group_added:
            return

        # In some flows, we may not have access to the actual link group lib
        # in our dep tree (e.g. https://fburl.com/code/pddmkptb), so just bail
        # in this case.
        # NOTE(agallagher): This case seems broken, as we're not going to set
        # DT_NEEDED tag correctly, or detect missing syms at link time.
        link_group_lib = link_group_libs.get(target_group)
        if link_group_lib == None:
            return
        _, shared_link_infos = link_group_lib

        expect(target_group != link_group)
        link_group_added[target_group] = None
        linkable_map[target] = LinkGroupLinkInfo(
            link_info = get_link_info_from_link_infos(shared_link_infos),
            output_style = LibOutputStyle("shared_lib"),
        )  # buildifier: disable=uninitialized

    filtered_groups = [None, NO_MATCH_LABEL, MATCH_ALL_LABEL]

    for target in linkables:
        node = linkable_graph_node_map[target]
        output_style = get_lib_output_style(link_strategy, link_group_preferred_linkage.get(target, node.preferred_linkage), pic_behavior)

        # Always link any shared dependencies
        if output_style == LibOutputStyle("shared_lib"):
            # filter out any dependencies to be discarded
            group = link_groups.get(link_group_mappings.get(target))
            if group != None and group.attrs.discard_group:
                continue

            # If this target is a link group root library, we
            # 1) don't propagate shared linkage down the tree, and
            # 2) use the provided link info in lieu of what's in the grph.
            target_link_group = link_group_roots.get(target)
            if target_link_group != None and target_link_group != link_group:
                add_link_group(target, target_link_group)
            else:
                add_link(target, LibOutputStyle("shared_lib"))
        else:  # static or static_pic
            target_link_group = link_group_mappings.get(target)

            # Always add force-static libs to the link.
            if force_static_follows_dependents and node.preferred_linkage == Linkage("static"):
                add_link(target, output_style)
            elif not target_link_group and not link_group:
                # Ungrouped linkable targets belong to the unlabeled executable
                add_link(target, output_style)
            elif is_executable_link and target_link_group == NO_MATCH_LABEL:
                # Targets labeled NO_MATCH belong to the unlabeled executable
                add_link(target, output_style)
            elif target_link_group == MATCH_ALL_LABEL or target_link_group == link_group:
                # If this belongs to the match all link group or the group currently being evaluated
                add_link(target, output_style)
            elif target_link_group not in filtered_groups:
                add_link_group(target, target_link_group)

    return linkable_map

# Find all link group libraries that are first order deps or exported deps of
# the exectuble or another link group's libs
def get_public_link_group_nodes(
        linkable_graph_node_map: dict[Label, LinkableNode],
        link_group_mappings: [dict[Label, str], None],
        executable_deps: list[Label],
        root_link_group: [str, None]) -> set_record:
    external_link_group_nodes = set()

    # TODO(@christylee): do we need to traverse root link group and NO_MATCH_LABEL exported deps?
    # buildifier: disable=uninitialized
    def crosses_link_group_boundary(current_group: [str, None], new_group: [str, None]):
        # belongs to root binary
        if new_group == root_link_group:
            return False

        if new_group == NO_MATCH_LABEL:
            # Using NO_MATCH with an explicitly defined root_link_group is undefined behavior
            expect(root_link_group == None or root_link_group == NO_MATCH_LABEL)
            return False

        # private node in link group
        if new_group == current_group:
            return False
        return True

    # Check the direct deps of the executable since the executable is not in linkable_graph_node_map
    for label in executable_deps:
        group = link_group_mappings.get(label)
        if crosses_link_group_boundary(root_link_group, group):
            external_link_group_nodes.add(label)

    # identify all nodes with a dependent across group boundaries
    for label, node in linkable_graph_node_map.items():
        current_group = link_group_mappings.get(label)

        for dep in node.deps + node.exported_deps:
            new_group = link_group_mappings.get(dep)
            if crosses_link_group_boundary(current_group, new_group):
                external_link_group_nodes.add(dep)

    SPECIAL_LINK_GROUPS = [MATCH_ALL_LABEL, NO_MATCH_LABEL]

    # Additionally identify exported_deps of those marked nodes (except those included in all groups or included in the main executable).

    # TODO(@christylee): dlopen-able libs that depend on the main executable does not have a
    # linkable internal edge to the main executable. Symbols that are not referenced during the
    # executable link might be dropped unless the dlopen-able libs are linked against the main
    # executable. We need to force export those symbols to avoid undefined symbls.

    # buildifier: disable=uninitialized
    def discover_link_group_linkables(node: Label) -> list[Label]:
        exported_deps = []
        for exported_dep in linkable_graph_node_map[node].exported_deps:
            group = link_group_mappings.get(exported_dep)
            if group != root_link_group and group not in SPECIAL_LINK_GROUPS:
                exported_deps.append(exported_dep)
        return exported_deps

    external_link_group_nodes.update(
        # get transitive exported deps
        breadth_first_traversal_by(
            linkable_graph_node_map,
            external_link_group_nodes.list(),
            discover_link_group_linkables,
        ),
    )

    return external_link_group_nodes

def get_filtered_links(
        labels_to_links_map: dict[Label, LinkGroupLinkInfo],
        public_link_group_nodes: [set_record, None] = None) -> list[LinkInfo]:
    if public_link_group_nodes == None:
        return [link_group_info.link_info for link_group_info in labels_to_links_map.values()]
    infos = []
    for label, link_group_info in labels_to_links_map.items():
        info = link_group_info.link_info
        if public_link_group_nodes.contains(label):
            infos.append(set_link_info_link_whole(info))
        else:
            infos.append(info)
    return infos

def get_filtered_targets(labels_to_links_map: dict[Label, LinkGroupLinkInfo]):
    return [label.raw_target() for label in labels_to_links_map.keys()]

def get_link_group_map_json(ctx: AnalysisContext, targets: list[TargetLabel]) -> DefaultInfo:
    json_map = ctx.actions.write_json(LINK_GROUP_MAP_DATABASE_FILENAME, sorted(targets))
    return DefaultInfo(default_output = json_map)

def find_relevant_roots(
        link_group: [str, None] = None,
        linkable_graph_node_map: dict[Label, LinkableNode] = {},
        link_group_mappings: dict[Label, str] = {},
        roots: list[Label] = []):
    # Walk through roots looking for the first node which maps to the current
    # link group.
    def collect_and_traverse_roots(roots, node_target):
        node = linkable_graph_node_map.get(node_target)
        if node.preferred_linkage == Linkage("static"):
            return node.deps + node.exported_deps
        node_link_group = link_group_mappings.get(node_target)
        if node_link_group == MATCH_ALL_LABEL:
            roots.append(node_target)
            return []
        if node_link_group == link_group:
            roots.append(node_target)
            return []
        return node.deps + node.exported_deps

    relevant_roots = []

    breadth_first_traversal_by(
        linkable_graph_node_map,
        roots,
        partial(collect_and_traverse_roots, relevant_roots),
    )

    return relevant_roots

def _create_link_group(
        ctx: AnalysisContext,
        spec: LinkGroupLibSpec,
        # The deps of the top-level executable.
        executable_deps: list[Label] = [],
        # Additional roots involved in the link.
        other_roots: list[Label] = [],
        public_nodes: set_record = set(),
        linkable_graph_node_map: dict[Label, LinkableNode] = {},
        linker_flags: list[typing.Any] = [],
        link_groups: dict[str, Group] = {},
        link_group_mappings: dict[Label, str] = {},
        link_group_preferred_linkage: dict[Label, Linkage] = {},
        link_strategy: LinkStrategy = LinkStrategy("static_pic"),
        link_group_libs: dict[str, ([Label, None], LinkInfos)] = {},
        prefer_stripped_objects: bool = False,
        category_suffix: [str, None] = None,
        anonymous: bool = False) -> [LinkedObject, None]:
    """
    Link a link group library, described by a `LinkGroupLibSpec`.  This is
    intended to handle regular shared libs and e.g. Python extensions.
    """

    inputs = []

    # Add extra linker flags.
    linker_info = get_cxx_toolchain_info(ctx).linker_info
    linker_type = linker_info.type
    inputs.append(LinkInfo(
        pre_flags =
            linker_flags +
            # There's scenarios where we no required syms won't be on the root
            # link lines (e.g. syms from executable), so we need to ignore
            # undefined symbols.
            get_ignore_undefined_symbols_flags(linker_type),
    ))

    # Get roots to begin the linkable search.
    # TODO(agallagher): We should use the groups "public" nodes as the roots.
    roots = []
    has_empty_root = False
    if spec.root != None:
        # If there's a linkable root attached to the spec, use that to guide
        # linking, as that will contain things like private linker flags that
        # might be required to link the given link group.
        inputs.append(get_link_info_from_link_infos(
            spec.root.link_infos,
            prefer_stripped = prefer_stripped_objects,
        ))
        roots.extend(spec.root.deps)
    else:
        for mapping in spec.group.mappings:
            # If there's no explicit root, this means we need to search the entire
            # graph to find candidate nodes.
            if mapping.root == None:
                has_empty_root = True
            else:
                roots.append(mapping.root)

        # If this link group has an empty mapping, we need to search everything
        # -- even the additional roots -- to find potential nodes in the link
        # group.
        if has_empty_root:
            roots.extend(
                find_relevant_roots(
                    link_group = spec.group.name,
                    linkable_graph_node_map = linkable_graph_node_map,
                    link_group_mappings = link_group_mappings,
                    roots = executable_deps + other_roots,
                ),
            )

    # Add roots...
    filtered_labels_to_links_map = get_filtered_labels_to_links_map(
        linkable_graph_node_map,
        spec.group.name,
        link_groups,
        link_group_mappings,
        link_group_preferred_linkage,
        pic_behavior = get_cxx_toolchain_info(ctx).pic_behavior,
        link_group_libs = link_group_libs,
        link_strategy = link_strategy,
        roots = roots,
        is_executable_link = False,
        prefer_stripped = prefer_stripped_objects,
    )
    inputs.extend(get_filtered_links(filtered_labels_to_links_map, public_nodes))

    if not filtered_labels_to_links_map and not spec.root:
        # don't create empty shared libraries
        return None

    # link the rule
    link_result = cxx_link_shared_library(
        ctx = ctx,
        output = paths.join("__link_groups__", spec.name),
        name = spec.name if spec.is_shared_lib else None,
        opts = link_options(
            links = [LinkArgs(infos = inputs)],
            category_suffix = category_suffix,
            identifier = spec.name,
            # TODO: anonymous targets cannot be used with dynamic output yet
            enable_distributed_thinlto = False if anonymous else spec.group.attrs.enable_distributed_thinlto,
            link_execution_preference = LinkExecutionPreference("any"),
        ),
        anonymous = anonymous,
    )
    return link_result.linked_object

def _stub_library(
        ctx: AnalysisContext,
        name: str,
        extra_ldflags: list[typing.Any] = [],
        anonymous: bool = False) -> LinkInfos:
    link_result = cxx_link_shared_library(
        ctx = ctx,
        output = name + ".stub",
        name = name,
        opts = link_options(
            links = [LinkArgs(flags = extra_ldflags)],
            identifier = name,
            category_suffix = "stub_library",
            link_execution_preference = LinkExecutionPreference("any"),
        ),
        anonymous = anonymous,
    )
    toolchain_info = get_cxx_toolchain_info(ctx)
    linker_info = toolchain_info.linker_info
    return LinkInfos(
        # Since we link against empty stub libraries, `--as-needed` will end up
        # removing `DT_NEEDED` tags that we actually need at runtime, so pass in
        # `--no-as-needed` last to make sure this is overridden.
        # TODO(agallagher): It'd be nice to at least support a mode where we
        # don't need to use empty stub libs.
        default = wrap_with_no_as_needed_shared_libs_flags(
            linker_type = linker_info.type,
            link_info = LinkInfo(
                linkables = [SharedLibLinkable(lib = link_result.linked_object.output)],
            ),
        ),
    )

def _symbol_files_for_link_group(
        ctx: AnalysisContext,
        lib: LinkedObject,
        prefer_local: bool = False,
        anonymous: bool = False) -> (Artifact, Artifact):
    """
    Find and return all undefined and global symbols form the given library.
    """

    # Extract undefined symbols.
    undefined_symfile = extract_undefined_syms(
        ctx = ctx,
        cxx_toolchain = get_cxx_toolchain_info(ctx),
        output = lib.output,
        category_prefix = "link_groups",
        prefer_local = prefer_local,
        anonymous = anonymous,
    )

    # Extract global symbols.
    global_symfile = extract_global_syms(
        ctx = ctx,
        cxx_toolchain = get_cxx_toolchain_info(ctx),
        output = lib.output,
        category_prefix = "link_groups",
        prefer_local = prefer_local,
        anonymous = anonymous,
    )

    return undefined_symfile, global_symfile

def _symbol_flags_for_link_groups(
        ctx: AnalysisContext,
        undefined_symfiles: list[Artifact] = [],
        global_symfiles: list[Artifact] = []) -> list[ArgLike]:
    """
    Generate linker flags which, when applied to the main executable, make sure
    required symbols are included in the link *and* exported to the dynamic
    symbol table.
    """

    sym_linker_flags = []

    # Format undefined symbols into an argsfile with `-u`s, and add to linker
    # flags.
    sym_linker_flags.append(
        get_undefined_symbols_args(
            ctx = ctx,
            cxx_toolchain = get_cxx_toolchain_info(ctx),
            name = "undefined_symbols.linker_script",
            symbol_files = undefined_symfiles,
            category = "link_groups_undefined_syms_script",
        ),
    )

    # Format global symfiles into a dynamic list version file, and add to
    # linker flags.
    dynamic_list_vers = create_dynamic_list_version_script(
        actions = ctx.actions,
        name = "dynamic_list.vers",
        symbol_files = global_symfiles,
        category = "link_groups_dynamic_list",
    )
    sym_linker_flags.extend([
        "-Wl,--dynamic-list",
        dynamic_list_vers,
    ])

    return sym_linker_flags

def create_link_groups(
        ctx: AnalysisContext,
        link_groups: dict[str, Group] = {},
        link_group_specs: list[LinkGroupLibSpec] = [],
        executable_deps: list[Label] = [],
        other_roots: list[Label] = [],
        root_link_group: [str, None] = None,
        linker_flags: list[typing.Any] = [],
        prefer_stripped_objects: bool = False,
        linkable_graph_node_map: dict[Label, LinkableNode] = {},
        link_group_preferred_linkage: dict[Label, Linkage] = {},
        link_group_mappings: [dict[Label, str], None] = None,
        anonymous: bool = False) -> _LinkedLinkGroups:
    # Generate stubs first, so that subsequent links can link against them.
    link_group_shared_links = {}
    specs = []
    for link_group_spec in link_group_specs:
        if link_group_spec.group.attrs.discard_group:
            # Don't create a link group for deps that we want to drop
            continue
        specs.append(link_group_spec)

        # always add libraries that are not constructed from link groups
        if link_group_spec.is_shared_lib:
            link_group_shared_links[link_group_spec.group.name] = _stub_library(
                ctx = ctx,
                name = link_group_spec.name,
                extra_ldflags = linker_flags,
                anonymous = anonymous,
            )

    linked_link_groups = {}
    undefined_symfiles = []
    global_symfiles = []

    public_nodes = get_public_link_group_nodes(
        linkable_graph_node_map,
        link_group_mappings,
        executable_deps + other_roots,
        root_link_group,
    )

    for link_group_spec in specs:
        # NOTE(agallagher): It might make sense to move this down to be
        # done when we generated the links for the executable, so we can
        # handle the case when a link group can depend on the executable.
        link_group_lib = _create_link_group(
            ctx = ctx,
            spec = link_group_spec,
            executable_deps = executable_deps,
            other_roots = other_roots,
            linkable_graph_node_map = linkable_graph_node_map,
            public_nodes = public_nodes,
            linker_flags = (
                linker_flags +
                link_group_spec.group.attrs.exported_linker_flags +
                link_group_spec.group.attrs.linker_flags
            ),
            link_groups = link_groups,
            link_group_mappings = link_group_mappings,
            link_group_preferred_linkage = link_group_preferred_linkage,
            # TODO(agallagher): Should we support alternate link strategies
            # (e.g. bottom-up with symbol errors)?
            link_group_libs = {
                name: (None, lib)
                for name, lib in link_group_shared_links.items()
            },
            prefer_stripped_objects = prefer_stripped_objects,
            category_suffix = "link_group",
            anonymous = anonymous,
        )

        if link_group_lib == None:
            # the link group did not match anything, don't create shlib interface
            continue

        # On GNU, use shlib interfaces.
        if cxx_is_gnu(ctx):
            shlib_for_link = shared_library_interface(
                ctx = ctx,
                shared_lib = link_group_lib.output,
                anonymous = anonymous,
            )
        else:
            shlib_for_link = link_group_lib.output

        link_info = LinkInfo(
            linkables = [SharedLibLinkable(lib = shlib_for_link)],
        )
        linked_link_groups[link_group_spec.group.name] = _LinkedLinkGroup(
            artifact = link_group_lib,
            library = None if not link_group_spec.is_shared_lib else LinkGroupLib(
                shared_libs = {link_group_spec.name: link_group_lib},
                shared_link_infos = LinkInfos(
                    default = wrap_link_info(
                        link_info,
                        pre_flags = link_group_spec.group.attrs.exported_linker_flags,
                    ),
                ),
            ),
        )

        # Merge and format all symbol files into flags that we can pass into
        # the binary link.
        undefined_symfile, global_symfile = _symbol_files_for_link_group(
            ctx = ctx,
            lib = link_group_lib,
            anonymous = anonymous,
        )
        undefined_symfiles.append(undefined_symfile)
        global_symfiles.append(global_symfile)

    # Add linker flags to make sure any symbols from the main executable
    # needed by these link groups are pulled in and exported to the dynamic
    # symbol table.
    symbol_ldflags = []
    if linked_link_groups:
        symbol_ldflags.extend(
            _symbol_flags_for_link_groups(
                ctx = ctx,
                undefined_symfiles = undefined_symfiles,
                global_symfiles = global_symfiles,
            ),
        )

    return _LinkedLinkGroups(
        libs = linked_link_groups,
        symbol_ldflags = symbol_ldflags,
    )

def get_transitive_deps_matching_labels(
        linkable_graph_node_map: dict[Label, LinkableNode],
        roots: list[Label],
        label: str) -> list[Label]:
    # NOTE: Our Haskell DLL support impl currently links transitive haskell
    # deps needed by DLLs which get linked into the main executable as link-
    # whole.  To emulate this, we mark Haskell rules with a special label
    # and traverse this to find all the nodes we need to link whole.
    nodes = []
    for dep in get_transitive_deps(linkable_graph_node_map, roots):
        linkable = linkable_graph_node_map[dep]
        if label not in linkable.labels:
            continue
        nodes.append(dep)
    return nodes
