# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//cxx:cxx_toolchain_types.bzl", "PicBehavior")
load("@prelude//cxx:headers.bzl", "CPrecompiledHeaderInfo")
load("@prelude//python:python.bzl", "PythonLibraryInfo")
load(
    "@prelude//utils:graph_utils.bzl",
    "breadth_first_traversal_by",
)
load("@prelude//utils:utils.bzl", "expect")
load(
    ":link_info.bzl",
    "LibOutputStyle",
    "LinkInfo",  # @unused Used as a type
    "LinkInfos",
    "LinkStrategy",
    "Linkage",
    "LinkedObject",
    "LinkerFlags",
    "MergedLinkInfo",
    "get_lib_output_style",
    "get_output_styles_for_linkage",
    _get_link_info = "get_link_info",
)

# A provider with information used to link a rule into a shared library.
# Potential omnibus roots must provide this so that omnibus can link them
# here, in the context of the top-level packaging rule.
LinkableRootInfo = provider(
    # @unsorted-dict-items
    fields = {
        "link_infos": provider_field(typing.Any, default = None),  # LinkInfos
        "name": provider_field(typing.Any, default = None),  # [str, None]
        "deps": provider_field(typing.Any, default = None),  # ["label"]
    },
)

###############################################################################
# Linkable Graph collects information on a node in the target graph that
# contains linkable output. This graph information may then be provided to any
# consumers of this target.
###############################################################################

_DisallowConstruction = record()

LinkableNode = record(
    # Attribute labels on the target.
    labels = field(list[str], []),
    # Preferred linkage for this target.
    preferred_linkage = field(Linkage, Linkage("any")),
    # Linkable deps of this target.
    deps = field(list[Label], []),
    # Exported linkable deps of this target.
    #
    # We distinguish between deps and exported deps so that when creating shared
    # libraries in a large graph we only need to link each library against its
    # deps and their (transitive) exported deps. This helps keep link lines smaller
    # and produces more efficient libs (for example, DT_NEEDED stays a manageable size).
    exported_deps = field(list[Label], []),
    # Link infos for all supported lib output styles supported by this node. This should have a value
    # for every output_style supported by the preferred linkage.
    link_infos = field(dict[LibOutputStyle, LinkInfos], {}),
    # Contains the linker flags for this node.
    # Note: The values in link_infos will already be adding in the exported_linker_flags
    # TODO(cjhopman): We should probably make all use of linker_flags explicit, but that may need to wait
    # for all link strategies to operate on the LinkableGraph.
    linker_flags = field(LinkerFlags),

    # Shared libraries provided by this target.  Used if this target is
    # excluded.
    shared_libs = field(dict[str, LinkedObject], {}),

    # The soname this node would use in default link strategies. May be used by non-default
    # link strategies as a lib's soname.
    default_soname = field(str | None),

    # Records Android's can_be_asset value for the node. This indicates whether the node can be bundled
    # as an asset in android apks.
    can_be_asset = field(bool),

    # Whether the node should appear in the android mergemap (which provides information about the original
    # soname->final merged lib mapping)
    include_in_android_mergemap = field(bool),

    # Only allow constructing within this file.
    _private = _DisallowConstruction,
)

LinkableGraphNode = record(
    # Target/label of this node
    label = field(Label),

    # If this node has linkable output, it's linkable data
    linkable = field([LinkableNode, None]),

    # All potential root notes for an omnibus link (e.g. C++ libraries,
    # C++ Python extensions).
    roots = field(dict[Label, LinkableRootInfo]),

    # Exclusions this node adds to the Omnibus graph
    excluded = field(dict[Label, None]),

    # Only allow constructing within this file.
    _private = _DisallowConstruction,
)

LinkableGraphTSet = transitive_set()

# The LinkableGraph for a target holds all the transitive nodes, roots, and exclusions
# from all of its dependencies.
#
# TODO(cjhopman): Rather than flattening this at each node, we should build up an actual
# graph structure.
LinkableGraph = provider(fields = {
    # Target identifier of the graph.
    "label": provider_field(typing.Any, default = None),  # Label
    "nodes": provider_field(typing.Any, default = None),  # "LinkableGraphTSet"
})

# Used to tag a rule as providing a shared native library that may be loaded
# dynamically, at runtime (e.g. via `dlopen`).
DlopenableLibraryInfo = provider(fields = {})

def _get_required_outputs_for_linkage(linkage: Linkage) -> list[LibOutputStyle]:
    if linkage == Linkage("shared"):
        return [LibOutputStyle("shared_lib")]

    return get_output_styles_for_linkage(linkage)

def create_linkable_node(
        ctx: AnalysisContext,
        default_soname: str | None,
        preferred_linkage: Linkage = Linkage("any"),
        deps: list[Dependency] = [],
        exported_deps: list[Dependency] = [],
        link_infos: dict[LibOutputStyle, LinkInfos] = {},
        shared_libs: dict[str, LinkedObject] = {},
        can_be_asset: bool = True,
        include_in_android_mergemap: bool = True,
        linker_flags: [LinkerFlags, None] = None) -> LinkableNode:
    for output_style in _get_required_outputs_for_linkage(preferred_linkage):
        expect(
            output_style in link_infos,
            "must have {} link info".format(output_style),
        )
    if not linker_flags:
        linker_flags = LinkerFlags()
    return LinkableNode(
        labels = ctx.attrs.labels,
        preferred_linkage = preferred_linkage,
        deps = linkable_deps(deps),
        exported_deps = linkable_deps(exported_deps),
        link_infos = link_infos,
        shared_libs = shared_libs,
        can_be_asset = can_be_asset,
        include_in_android_mergemap = include_in_android_mergemap,
        default_soname = default_soname,
        linker_flags = linker_flags,
        _private = _DisallowConstruction(),
    )

def create_linkable_graph_node(
        ctx: AnalysisContext,
        linkable_node: [LinkableNode, None] = None,
        roots: dict[Label, LinkableRootInfo] = {},
        excluded: dict[Label, None] = {}) -> LinkableGraphNode:
    return LinkableGraphNode(
        label = ctx.label,
        linkable = linkable_node,
        roots = roots,
        excluded = excluded,
        _private = _DisallowConstruction(),
    )

def create_linkable_graph(
        ctx: AnalysisContext,
        node: [LinkableGraphNode, None] = None,
        # This list of deps must include all deps referenced by the LinkableGraphNode.
        deps: list[[LinkableGraph, Dependency]] = []) -> LinkableGraph:
    graph_deps = []
    for d in deps:
        if eval_type(LinkableGraph.type).matches(d):
            graph_deps.append(d)
        else:
            graph = d.get(LinkableGraph)
            if graph:
                graph_deps.append(graph)

    deps_labels = {x.label: True for x in graph_deps}
    if node and node.linkable:
        for l in [node.linkable.deps, node.linkable.exported_deps]:
            for d in l:
                if not d in deps_labels:
                    fail("LinkableNode had {} in its deps, but that label is missing from the node's linkable graph children (`{}`)".format(d, ", ".join(deps_labels)))

    children = [x.nodes for x in graph_deps]

    kwargs = {
        "children": children,
    }
    if node:
        kwargs["value"] = node
    return LinkableGraph(
        label = ctx.label,
        nodes = ctx.actions.tset(LinkableGraphTSet, **kwargs),
    )

def get_linkable_graph_node_map_func(graph: LinkableGraph):
    def get_linkable_graph_node_map() -> dict[Label, LinkableNode]:
        nodes = graph.nodes.traverse()
        linkable_nodes = {}
        for node in filter(None, nodes):
            if node.linkable:
                linkable_nodes[node.label] = node.linkable
        return linkable_nodes

    return get_linkable_graph_node_map

def linkable_deps(deps: list[Dependency]) -> list[Label]:
    labels = []

    for dep in deps:
        dep_info = linkable_graph(dep)
        if dep_info != None:
            labels.append(dep_info.label)

    return labels

def linkable_graph(dep: Dependency) -> [LinkableGraph, None]:
    """
    Helper to extract `LinkableGraph` from a dependency which also
    provides `MergedLinkInfo`.
    """

    # We only care about "linkable" deps.
    if PythonLibraryInfo in dep or MergedLinkInfo not in dep or dep.label.sub_target == ["headers"]:
        return None

    if CPrecompiledHeaderInfo in dep:
        # `cxx_precompiled_header()` does not contribute to the link, only to compile
        return None

    expect(
        LinkableGraph in dep,
        "{} provides `MergedLinkInfo`".format(dep.label) +
        " but doesn't also provide `LinkableGraph`",
    )

    return dep[LinkableGraph]

def get_link_info(
        node: LinkableNode,
        output_style: LibOutputStyle,
        prefer_stripped: bool = False) -> LinkInfo:
    info = _get_link_info(
        node.link_infos[output_style],
        prefer_stripped = prefer_stripped,
    )
    return info

def get_deps_for_link(
        node: LinkableNode,
        strategy: LinkStrategy,
        pic_behavior: PicBehavior) -> list[Label]:
    """
    Return deps to follow when linking against this node with the given link
    style.
    """

    # Avoid making a copy of the list until we know have to modify it.
    deps = node.exported_deps

    # If we're linking statically, include non-exported deps.
    output_style = get_lib_output_style(strategy, node.preferred_linkage, pic_behavior)
    if output_style != LibOutputStyle("shared_lib") and node.deps:
        # Important that we don't mutate deps, but create a new list
        deps = deps + node.deps

    return deps

def get_transitive_deps(
        link_infos: dict[Label, LinkableNode],
        roots: list[Label]) -> list[Label]:
    """
    Return all transitive deps from following the given nodes.
    """

    def find_transitive_deps(node: Label):
        return link_infos[node].deps + link_infos[node].exported_deps

    all_deps = breadth_first_traversal_by(link_infos, roots, find_transitive_deps)

    return all_deps
