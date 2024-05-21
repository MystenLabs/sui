# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//decls:re_test_common.bzl", "re_test_common")
load("@prelude//tests:remote_test_execution_toolchain.bzl", "RemoteTestExecutionToolchainInfo")
load("@prelude//utils:utils.bzl", "map_val")

def _impl(ctx: AnalysisContext) -> list[Provider]:
    return [
        DefaultInfo(),
        RemoteTestExecutionToolchainInfo(
            default_profile = map_val(ctx.attrs.profiles.get, ctx.attrs.default_profile),
            profiles = ctx.attrs.profiles,
        ),
    ]

remote_test_execution_toolchain = rule(
    impl = _impl,
    is_toolchain_rule = True,
    attrs = {
        "default_profile": attrs.option(attrs.string(), default = None),
        "profiles": attrs.dict(
            key = attrs.string(),
            value = attrs.option(re_test_common.opts_for_tests_arg()),
            default = {},
        ),
    },
)
