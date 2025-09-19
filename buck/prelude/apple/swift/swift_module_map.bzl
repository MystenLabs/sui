# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//utils:arglike.bzl", "ArgLike")  # @unused Used as a type
load(":swift_toolchain_types.bzl", "SwiftCompiledModuleTset")

def write_swift_module_map(
        ctx: AnalysisContext,
        module_name: str,
        sdk_deps: SwiftCompiledModuleTset) -> ArgLike:
    return write_swift_module_map_with_swift_deps(ctx, module_name, sdk_deps, None)

def write_swift_module_map_with_swift_deps(
        ctx: AnalysisContext,
        module_name: str,
        sdk_swift_deps: SwiftCompiledModuleTset,
        swift_deps: [SwiftCompiledModuleTset, None]) -> ArgLike:
    if swift_deps:
        all_deps = ctx.actions.tset(SwiftCompiledModuleTset, children = [sdk_swift_deps, swift_deps])
    else:
        all_deps = sdk_swift_deps

    return ctx.actions.write_json(
        module_name + ".swift_module_map.json",
        all_deps.project_as_json("swift_module_map"),
        with_inputs = True,
    )
