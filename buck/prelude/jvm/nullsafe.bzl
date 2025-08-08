# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//java:java_toolchain.bzl", "JavaToolchainInfo")
load("@prelude//java/plugins:java_plugin.bzl", "PluginParams", "create_plugin_params")

NullsafeInfo = record(
    output = field(Artifact),
    plugin_params = field(PluginParams),
    extra_arguments = field(cmd_args),
)

def get_nullsafe_info(
        ctx: AnalysisContext) -> [NullsafeInfo, None]:
    java_toolchain = ctx.attrs._java_toolchain[JavaToolchainInfo]
    extra_arguments = cmd_args(ctx.attrs.extra_arguments)

    nullsafe_plugin = java_toolchain.nullsafe
    nullsafe_signatures = java_toolchain.nullsafe_signatures
    nullsafe_extra_args = java_toolchain.nullsafe_extra_args

    if nullsafe_plugin:
        nullsafe_output = ctx.actions.declare_output("reports", dir = True)
        nullsafe_plugin_params = create_plugin_params(ctx, [nullsafe_plugin])

        nullsafe_args = cmd_args(
            "-XDcompilePolicy=simple",
            "-Anullsafe.reportToJava=false",
        )
        nullsafe_args.add(cmd_args(
            nullsafe_output.as_output(),
            format = "-Anullsafe.writeJsonReportToDir={}",
        ))
        if nullsafe_signatures:
            nullsafe_args.add(cmd_args(
                nullsafe_signatures,
                format = "-Anullsafe.signatures={}",
            ))
        if nullsafe_extra_args:
            nullsafe_args.add(nullsafe_extra_args)

        extra_arguments.add(nullsafe_args)

        return NullsafeInfo(
            output = nullsafe_output,
            plugin_params = nullsafe_plugin_params,
            extra_arguments = extra_arguments,
        )

    return None
