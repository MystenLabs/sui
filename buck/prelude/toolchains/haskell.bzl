# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//haskell:haskell.bzl", "HaskellPlatformInfo", "HaskellToolchainInfo")

def _system_haskell_toolchain(_ctx: AnalysisContext) -> list[Provider]:
    return [
        DefaultInfo(),
        HaskellToolchainInfo(
            compiler = "ghc",
            packager = "ghc-pkg",
            linker = "ghc",
            compiler_flags = [],
            linker_flags = [],
        ),
        HaskellPlatformInfo(
            name = "x86_64",
        ),
    ]

system_haskell_toolchain = rule(
    impl = _system_haskell_toolchain,
    attrs = {},
    is_toolchain_rule = True,
)
