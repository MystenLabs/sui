# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# split dwarf targets have ["dwp"] subtargets. this function ensures that the dwp file is materialized
def ensure_dwp(ctx: bxl.Context, target: bxl.ConfiguredTargetNode):
    providers = ctx.analysis(target).providers()
    subtargets = providers[DefaultInfo].sub_targets

    if "dwp" in subtargets:
        ctx.output.ensure(subtargets["dwp"][DefaultInfo].default_outputs[0])
