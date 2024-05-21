# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# BOLT (Binary Optimization Layout Tool) is a post link profile guided optimizer used for
# performance-critical services in fbcode: https://www.internalfb.com/intern/wiki/HHVM-BOLT/

load(":cxx_context.bzl", "get_cxx_toolchain_info")

def cxx_use_bolt(ctx: AnalysisContext) -> bool:
    cxx_toolchain_info = get_cxx_toolchain_info(ctx)
    return cxx_toolchain_info.bolt_enabled and ctx.attrs.bolt_profile != None

def bolt(ctx: AnalysisContext, prebolt_output: Artifact, identifier: [str, None]) -> Artifact:
    output_name = prebolt_output.short_path.removesuffix("-wrapper")
    postbolt_output = ctx.actions.declare_output(output_name)
    bolt_msdk = get_cxx_toolchain_info(ctx).binary_utilities_info.bolt_msdk

    if not bolt_msdk or not cxx_use_bolt(ctx):
        fail("Cannot use bolt if bolt_msdk is not available or bolt profile is not available")
    args = cmd_args()

    # bolt command format:
    # {llvm_bolt} {input_bin} -o $OUT -data={fdata} {args}
    args.add(
        cmd_args(bolt_msdk, format = "{}/bin/llvm-bolt"),
        prebolt_output,
        "-o",
        postbolt_output.as_output(),
        cmd_args(ctx.attrs.bolt_profile, format = "-data={}"),
        ctx.attrs.bolt_flags,
    )

    ctx.actions.run(
        args,
        category = "bolt",
        identifier = identifier,
        local_only = get_cxx_toolchain_info(ctx).linker_info.link_binaries_locally,
    )

    return postbolt_output
