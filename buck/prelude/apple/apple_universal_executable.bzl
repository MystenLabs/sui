# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(
    "@prelude//:artifact_tset.bzl",
    "project_artifacts",
)
load(":apple_bundle_types.bzl", "AppleBundleLinkerMapInfo", "AppleMinDeploymentVersionInfo")
load(":apple_bundle_utility.bzl", "get_default_binary_dep", "get_flattened_binary_deps", "merge_bundle_linker_maps_info")
load(":apple_code_signing_types.bzl", "AppleEntitlementsInfo")
load(":apple_dsym.bzl", "DSYM_SUBTARGET", "get_apple_dsym_ext")
load(":apple_universal_binaries.bzl", "create_universal_binary")
load(":debug.bzl", "AppleDebuggableInfo")
load(":resource_groups.bzl", "ResourceGraphInfo")

_FORWARDED_PROVIDER_TYPES = [
    AppleMinDeploymentVersionInfo,
    AppleEntitlementsInfo,
    ResourceGraphInfo,
]

_MERGED_PROVIDER_TYPES = [
    AppleDebuggableInfo,
    AppleBundleLinkerMapInfo,
]

def _get_universal_binary_name(binary_deps: dict[str, Dependency]):
    # Because `binary_deps` is a split transition of the same target,
    # the filenames would be identical, so we just pick the first one.
    first_binary_dep = binary_deps.values()[0]
    first_binary_artifact = first_binary_dep[DefaultInfo].default_outputs[0]

    # The universal executable should have the same name as the base/thin ones
    return first_binary_artifact.short_path

def apple_universal_executable_impl(ctx: AnalysisContext) -> list[Provider]:
    dsym_name = ctx.attrs.name + ".dSYM"
    binary_outputs = create_universal_binary(
        ctx = ctx,
        binary_deps = ctx.attrs.executable,
        binary_name = _get_universal_binary_name(ctx.attrs.executable),
        dsym_bundle_name = dsym_name,
        split_arch_dsym = ctx.attrs.split_arch_dsym,
    )

    sub_targets = {}
    if ctx.attrs.split_arch_dsym:
        dsyms = binary_outputs.debuggable_info.dsyms
    else:
        dsyms = [get_apple_dsym_ext(
            ctx = ctx,
            executable = binary_outputs.binary,
            debug_info = project_artifacts(
                actions = ctx.actions,
                tsets = [binary_outputs.debuggable_info.debug_info_tset],
            ),
            action_identifier = ctx.attrs.name + "_dsym",
            output_path = dsym_name,
        )]
    sub_targets[DSYM_SUBTARGET] = [DefaultInfo(default_outputs = dsyms)]

    default_binary = get_default_binary_dep(ctx.attrs.executable)
    forwarded_providers = []
    for forward_provider_type in _FORWARDED_PROVIDER_TYPES:
        provider = default_binary.get(forward_provider_type)
        if provider != None:
            forwarded_providers += [provider]

    merged_providers = []
    all_binarys = get_flattened_binary_deps(ctx.attrs.executable)
    for merged_provider_type in _MERGED_PROVIDER_TYPES:
        if default_binary.get(merged_provider_type) == None:
            continue
        if merged_provider_type == AppleDebuggableInfo:
            merged_providers += [
                AppleDebuggableInfo(
                    dsyms = dsyms,
                    debug_info_tset = binary_outputs.debuggable_info.debug_info_tset,
                ),
            ]
        elif merged_provider_type == AppleBundleLinkerMapInfo:
            merged_providers += [
                merge_bundle_linker_maps_info([binary[AppleBundleLinkerMapInfo] for binary in all_binarys]),
            ]
        else:
            fail("Unhandled provider type: {}".format(merged_provider_type))

    return [
        DefaultInfo(default_output = binary_outputs.binary, sub_targets = sub_targets),
        RunInfo(args = cmd_args(binary_outputs.binary)),
    ] + forwarded_providers + merged_providers
