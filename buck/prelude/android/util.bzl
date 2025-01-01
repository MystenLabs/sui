# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# A wrapper around AnalysisContext that makes it easier to define subtargets without needing to pass information
# for them all the way back to the outermost analysis impl.
EnhancementContext = record(
    ctx = AnalysisContext,
    actions = AnalysisActions,
    attrs = typing.Any,
    label = Label,

    # methods
    debug_output = typing.Callable,
    get_sub_targets = typing.Callable,
)

def create_enhancement_context(ctx: AnalysisContext) -> EnhancementContext:
    extra_sub_targets = {}

    def debug_output(name: str, output: Artifact, other_outputs = []):
        """Adds a subtarget to expose debugging outputs."""
        extra_sub_targets[name] = [DefaultInfo(default_outputs = [output], other_outputs = other_outputs)]

    def get_sub_targets():
        return extra_sub_targets

    return EnhancementContext(
        ctx = ctx,
        actions = ctx.actions,
        attrs = ctx.attrs,
        label = ctx.label,
        debug_output = debug_output,
        get_sub_targets = get_sub_targets,
    )
