# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//toolchains:cxx.bzl", "system_cxx_toolchain")
load("@prelude//toolchains:genrule.bzl", "system_genrule_toolchain")
load("@prelude//toolchains:haskell.bzl", "system_haskell_toolchain")
load("@prelude//toolchains:ocaml.bzl", "system_ocaml_toolchain")
load("@prelude//toolchains:python.bzl", "system_python_bootstrap_toolchain", "system_python_toolchain")
load("@prelude//toolchains:remote_test_execution.bzl", "remote_test_execution_toolchain")
load("@prelude//toolchains:rust.bzl", "system_rust_toolchain")

def system_demo_toolchains():
    """
    All the default toolchains, suitable for a quick demo or early prototyping.
    Most real projects should copy/paste the implementation to configure them.
    """
    system_cxx_toolchain(
        name = "cxx",
        visibility = ["PUBLIC"],
    )

    system_genrule_toolchain(
        name = "genrule",
        visibility = ["PUBLIC"],
    )

    system_haskell_toolchain(
        name = "haskell",
        visibility = ["PUBLIC"],
    )

    system_ocaml_toolchain(
        name = "ocaml",
        visibility = ["PUBLIC"],
    )

    system_python_toolchain(
        name = "python",
        visibility = ["PUBLIC"],
    )

    system_python_bootstrap_toolchain(
        name = "python_bootstrap",
        visibility = ["PUBLIC"],
    )

    system_rust_toolchain(
        name = "rust",
        default_edition = "2021",
        visibility = ["PUBLIC"],
    )

    remote_test_execution_toolchain(
        name = "remote_test_execution",
        visibility = ["PUBLIC"],
    )
