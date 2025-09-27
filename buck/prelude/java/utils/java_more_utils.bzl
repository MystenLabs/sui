# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(
    "@prelude//os_lookup:defs.bzl",
    "OsLookup",  # @unused Used as type
)
load("@prelude//utils:utils.bzl", "expect")

def get_path_separator_for_exec_os(ctx: AnalysisContext) -> str:
    expect(hasattr(ctx.attrs, "_exec_os_type"), "Expect ctx.attrs._exec_os_type is defined.")
    is_windows = ctx.attrs._exec_os_type[OsLookup].platform == "windows"
    return ";" if is_windows else ":"
