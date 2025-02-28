# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//:genrule_types.bzl", "GENRULE_MARKER_SUBTARGET_NAME")

def get_apple_genrule_deps_outputs(deps: list[Dependency]) -> list[Artifact]:
    artifacts = []
    for dep in deps:
        default_info = dep[DefaultInfo]
        if GENRULE_MARKER_SUBTARGET_NAME in default_info.sub_targets:
            artifacts += default_info.default_outputs
    return artifacts

def get_apple_build_genrule_deps_attr_value(ctx: AnalysisContext) -> bool:
    build_genrule_deps = ctx.attrs.build_genrule_deps
    if build_genrule_deps != None:
        # `build_genrule_deps` present on a target takes priority
        return build_genrule_deps

    # Fallback to the default value which is driven by buckconfig + select()
    return ctx.attrs._build_genrule_deps

def get_apple_build_genrule_deps_default_kwargs() -> dict[str, typing.Any]:
    return {
        APPLE_BUILD_GENRULE_DEPS_DEFAULT_ATTRIB_NAME: _build_genrule_deps_default_enabled(),
    }

def _build_genrule_deps_default_enabled() -> typing.Any:
    buckconfig_value = read_root_config("apple", "build_genrule_deps", None)
    if buckconfig_value != None:
        return buckconfig_value.lower() == "true"

    return select({
        "DEFAULT": False,
        # TODO(mgd): Make `config//` references possible from macro layer
        "ovr_config//features/apple/constraints:build_genrule_deps_enabled": True,
    })

APPLE_BUILD_GENRULE_DEPS_DEFAULT_ATTRIB_NAME = "_build_genrule_deps"
APPLE_BUILD_GENRULE_DEPS_DEFAULT_ATTRIB_TYPE = attrs.bool(default = False)

APPLE_BUILD_GENRULE_DEPS_TARGET_ATTRIB_NAME = "build_genrule_deps"
APPLE_BUILD_GENRULE_DEPS_TARGET_ATTRIB_TYPE = attrs.option(attrs.bool(), default = None)
