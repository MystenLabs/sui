# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(
    "@prelude//cxx:groups.bzl",
    "BuildTargetFilter",  # @unused Used as a type
    "FilterType",
    "Group",  # @unused Used as a type
    "GroupMapping",  # @unused Used as a type
    "LabelFilter",  # @unused Used as a type
    "parse_groups_definitions",
)
load(
    "@prelude//cxx:link_groups.bzl",
    "LinkGroupInfo",
    "build_link_group_info",
)
load(
    "@prelude//linking:link_groups.bzl",
    "LinkGroupLibInfo",
)
load(
    "@prelude//linking:link_info.bzl",
    "MergedLinkInfo",
)
load(
    "@prelude//linking:linkable_graph.bzl",
    "LinkableGraph",
    "create_linkable_graph",
)
load(
    "@prelude//linking:shared_libraries.bzl",
    "SharedLibraryInfo",
)
load("@prelude//user:rule_spec.bzl", "RuleRegistrationSpec")
load(
    "@prelude//utils:build_target_pattern.bzl",
    "BuildTargetPattern",  # @unused Used as a type
)
load("@prelude//decls/common.bzl", "Linkage", "Traversal")

def _v1_attrs(
        optional_root: bool = False,
        # Whether we should parse `root` fields as a `dependency`, instead of a `label`.
        root_is_dep: bool = True):
    if root_is_dep:
        attrs_root = attrs.dep(providers = [
            LinkGroupLibInfo,
            LinkableGraph,
            MergedLinkInfo,
            SharedLibraryInfo,
        ])
    else:
        attrs_root = attrs.label()

    if optional_root:
        attrs_root = attrs.option(attrs_root)

    return attrs.list(
        attrs.tuple(
            # name
            attrs.string(),
            # list of mappings
            attrs.list(
                # a single mapping
                attrs.tuple(
                    # root node
                    attrs_root,
                    # traversal
                    attrs.enum(Traversal),
                    # filters, either `None`, a single filter, or a list of filters
                    # (which must all match).
                    attrs.option(attrs.one_of(attrs.list(attrs.string()), attrs.string())),
                    # linkage
                    attrs.option(attrs.enum(Linkage)),
                ),
            ),
            # attributes
            attrs.option(
                attrs.dict(key = attrs.string(), value = attrs.any(), sorted = False),
            ),
        ),
    )

def link_group_map_attr():
    v2_attrs = attrs.dep(providers = [LinkGroupInfo])
    return attrs.option(
        attrs.one_of(
            v2_attrs,
            _v1_attrs(
                optional_root = True,
                # Inlined `link_group_map` will parse roots as `label`s, to avoid
                # bloating deps w/ unrelated mappings (e.g. it's common to use
                # a default mapping for all rules, which would otherwise add
                # unrelated deps to them).
                root_is_dep = False,
            ),
        ),
        default = None,
    )

def _make_json_info_for_build_target_pattern(build_target_pattern: BuildTargetPattern) -> dict[str, typing.Any]:
    # `BuildTargetPattern` contains lambdas which are not serializable, so
    # have to generate the JSON representation
    return {
        "cell": build_target_pattern.cell,
        "kind": build_target_pattern.kind,
        "name": build_target_pattern.name,
        "path": build_target_pattern.path,
    }

def _make_json_info_for_group_mapping_filters(filters: list[[BuildTargetFilter, LabelFilter]]) -> list[dict[str, typing.Any]]:
    json_filters = []
    for filter in filters:
        if filter._type == FilterType("label"):
            json_filters += [{"regex": str(filter.regex)}]
        elif filter._type == FilterType("pattern"):
            json_filters += [_make_json_info_for_build_target_pattern(filter.pattern)]
        else:
            fail("Unknown filter type: " + filter)
    return json_filters

def _make_json_info_for_group_mapping(group_mapping: GroupMapping) -> dict[str, typing.Any]:
    return {
        "filters": _make_json_info_for_group_mapping_filters(group_mapping.filters),
        "preferred_linkage": group_mapping.preferred_linkage,
        "root": group_mapping.root,
        "traversal": group_mapping.traversal,
    }

def _make_json_info_for_group(group: Group) -> dict[str, typing.Any]:
    return {
        "attrs": group.attrs,
        "mappings": [_make_json_info_for_group_mapping(mapping) for mapping in group.mappings],
        "name": group.name,
    }

def _make_info_subtarget_providers(ctx: AnalysisContext, link_group_info: LinkGroupInfo) -> list[Provider]:
    info_json = {
        "groups": {name: _make_json_info_for_group(group) for name, group in link_group_info.groups.items()},
        "mappings": link_group_info.mappings,
    }
    json_output = ctx.actions.write_json("link_group_map_info.json", info_json)
    return [DefaultInfo(default_output = json_output)]

def _impl(ctx: AnalysisContext) -> list[Provider]:
    # Extract graphs from the roots via the raw attrs, as `parse_groups_definitions`
    # parses them as labels.
    linkable_graph = create_linkable_graph(
        ctx,
        deps = [
            mapping[0][LinkableGraph]
            for entry in ctx.attrs.map
            for mapping in entry[1]
        ],
    )
    link_groups = parse_groups_definitions(ctx.attrs.map, lambda root: root.label)
    link_group_info = build_link_group_info(linkable_graph, link_groups)
    return [
        DefaultInfo(sub_targets = {
            "info": _make_info_subtarget_providers(ctx, link_group_info),
        }),
        link_group_info,
    ]

registration_spec = RuleRegistrationSpec(
    name = "link_group_map",
    impl = _impl,
    attrs = {
        "map": _v1_attrs(),
    },
)
