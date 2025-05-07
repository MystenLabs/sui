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
    "project_artifacts",
)

DEBUGINFO_SUBTARGET = "debuginfo"
DEBUGINFO_DB_SUBTARGET = "debuginfo-db"

AppleDebugInfo = TransitiveSetArgsProjection

# Represents Apple debug info from both executables and bundles.
AppleDebuggableInfo = provider(
    # @unsorted-dict-items
    fields = {
        "dsyms": provider_field(list[Artifact]),
        # Tset containing ArtifactInfos with either
        # a. the owning library target to artifacts, or
        # b. the owning bundle target to filtered artifacts
        "debug_info_tset": provider_field(ArtifactTSet),
        # In the case of b above, contians the map of library target to artifacts, else None
        "filtered_map": provider_field([dict[Label, list[Artifact]], None], default = None),
    },
)

_AppleDebugInfo = record(
    debug_info_tset = "ArtifactTSet",
    filtered_map = field([dict[Label, list[Artifact]], None]),
)

AggregatedAppleDebugInfo = record(
    debug_info = field(_AppleDebugInfo),
    # debug_info_tset = field(ArtifactTSet),
    sub_targets = field(dict[str, list[DefaultInfo]]),
)

def get_aggregated_debug_info(ctx: AnalysisContext, debug_infos: list[AppleDebuggableInfo]) -> AggregatedAppleDebugInfo:
    all_debug_info_tsets = []
    full_debug_info_tsets = []
    debug_info_map = {}

    # If the AppleDebuggableInfo has a filtered map, the tset contains filtered info with a label equivalent to the bundle that propagated the
    # artifacts. Thus, we need to track whether any of the infos have a filtered map, and if so propagate the filtered map.
    any_info_has_filtered_map = False
    for info in debug_infos:
        all_debug_info_tsets.append(info.debug_info_tset)

        if info.filtered_map:
            debug_info_map.update(info.filtered_map)
            any_info_has_filtered_map = True
        else:
            full_debug_info_tsets.append(info.debug_info_tset)

    debug_info_tset = make_artifact_tset(
        actions = ctx.actions,
        label = ctx.label,
        children = all_debug_info_tsets,
    )
    sub_targets = {}
    sub_targets[DEBUGINFO_SUBTARGET] = [
        DefaultInfo(default_output = ctx.actions.write(
            "debuginfo.artifacts",
            project_artifacts(
                actions = ctx.actions,
                tsets = [debug_info_tset],
            ),
            with_inputs = True,
        )),
    ]

    full_debug_info_tset = make_artifact_tset(
        actions = ctx.actions,
        label = ctx.label,
        children = full_debug_info_tsets,
    )
    if full_debug_info_tset._tset:
        debug_info_map.update({info.label: info.artifacts for infos in full_debug_info_tset._tset.traverse() for info in infos})

    sub_targets[DEBUGINFO_DB_SUBTARGET] = [
        DefaultInfo(
            default_output = ctx.actions.write_json(DEBUGINFO_DB_SUBTARGET, debug_info_map),
        ),
    ]
    return AggregatedAppleDebugInfo(
        debug_info = _AppleDebugInfo(
            debug_info_tset = debug_info_tset,
            filtered_map = debug_info_map if any_info_has_filtered_map else None,
        ),
        sub_targets = sub_targets,
    )
