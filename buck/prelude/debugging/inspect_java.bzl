# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//debugging/common.bzl", "create_target_info", "target_name")
load("@prelude//debugging/types.bzl", "ExecInfo", "JavaInfo", "ScriptSettings", "TargetExtraInfo")
load("@prelude//java/class_to_srcs.bzl", "JavaClassToSourceMapInfo")

def inspect_java_rule(ctx: bxl.Context, _actions: AnalysisActions, target: bxl.ConfiguredTargetNode, settings: ScriptSettings) -> ExecInfo:
    providers = ctx.analysis(target).providers()
    debuginfo = providers[JavaClassToSourceMapInfo].debuginfo if JavaClassToSourceMapInfo in providers else None
    if debuginfo:
        ctx.output.ensure(debuginfo)

    return ExecInfo(
        target_name = target_name(settings.target),
        target_info = create_target_info(settings.target),
        data = TargetExtraInfo(
            exec_info_version = 1,
            debugger = "fdb:debugger:jdwp",
            java = JavaInfo(
                classmap_file = debuginfo,
            ),
        ),
    )
