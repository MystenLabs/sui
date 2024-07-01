# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//debugging/common.bzl", "create_target_info", "target_name")
load("@prelude//debugging/types.bzl", "JavaInfo", "ScriptSettings")
load("@prelude//java/class_to_srcs.bzl", "JavaClassToSourceMapInfo")

def inspect_dbg_exec(ctx: bxl.Context, actions: AnalysisActions, target: bxl.ConfiguredTargetNode, settings: ScriptSettings):
    pointer_name = target_name(target)
    if not pointer_name.endswith("_fdb"):
        pointer_name = "{}_fdb".format(pointer_name)

    fbsource_alias_target = ctx.configured_targets(pointer_name)
    providers = ctx.analysis(fbsource_alias_target).providers()
    fdb_helper = providers[RunInfo]
    fdb_helper_out = actions.declare_output("fdb_helper.json")
    cmd = cmd_args(fdb_helper)
    cmd.add(settings.args)
    actions.run(cmd, category = "fdb_helper", env = {"FDB_OUTPUT_FILE": fdb_helper_out.as_output()}, local_only = True)
    result = actions.declare_output("final_out.json")

    original_target_providers = ctx.analysis(settings.target).providers()
    java_debuginfo = original_target_providers[JavaClassToSourceMapInfo].debuginfo if JavaClassToSourceMapInfo in original_target_providers else None
    if java_debuginfo:
        ctx.output.ensure(java_debuginfo)

    def build_exec_info(ctx, artifacts, outputs):
        # TODO: make this more independent
        # java is only supported via [JavaClassToSourceMapInfo] provider
        exec_info = artifacts[fdb_helper_out].read_json()
        exec_info["java"] = JavaInfo(
            classmap_file = java_debuginfo,
        )

        # read_json can't create a record of type ExecInfo
        # can alternativelly create ExecInfo by enumerating every single primitive nested field in there
        ctx.bxl_actions().actions.write_json(outputs[result], {
            "data": exec_info,
            "target_info": create_target_info(settings.target),
            "target_name": target_name(settings.target),
        })

    actions.dynamic_output(
        dynamic = [fdb_helper_out],
        inputs = [],
        outputs = [result],
        f = build_exec_info,
    )
    return result
