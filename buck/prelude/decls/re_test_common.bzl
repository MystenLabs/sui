# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//:build_mode.bzl", "BuildModeInfo")
load("@prelude//:is_full_meta_repo.bzl", "is_full_meta_repo")
load(":toolchains_common.bzl", "toolchains_common")

def _opts_for_tests_arg() -> Attr:
    # Attributes types do not have records.
    # The expected shape of re_opts is:
    # {
    #     "capabilities": Dict<str, str> | None
    #     "use_case": str | None
    #     "remote_cache_enabled": bool | None
    # }
    return attrs.dict(
        key = attrs.string(),
        value = attrs.option(
            attrs.one_of(
                attrs.dict(
                    key = attrs.string(),
                    value = attrs.string(),
                    sorted = False,
                ),
                attrs.string(),
                attrs.bool(),
            ),
            # TODO(cjhopman): I think this default does nothing, it should be deleted
            default = None,
        ),
        sorted = False,
    )

def _action_key_provider_arg() -> Attr:
    if is_full_meta_repo():
        return attrs.dep(providers = [BuildModeInfo], default = "fbcode//buck2/platform/build_mode:build_mode")
    else:
        return attrs.option(attrs.dep(providers = [BuildModeInfo]), default = None)

def _test_args() -> dict[str, Attr]:
    return {
        "remote_execution": attrs.option(
            attrs.one_of(
                attrs.string(),
                _opts_for_tests_arg(),
            ),
            default = None,
        ),
        "remote_execution_action_key_providers": _action_key_provider_arg(),
        "_remote_test_execution_toolchain": toolchains_common.remote_test_execution_toolchain(),
    }

re_test_common = struct(
    test_args = _test_args,
    opts_for_tests_arg = _opts_for_tests_arg,
)
