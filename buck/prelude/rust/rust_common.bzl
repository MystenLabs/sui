# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(":with_workspace.bzl", "package_key")

def rust_common_macro_wrapper(rust_rule):
    def rust_common_impl(**kwargs):
        workspaces = read_package_value(package_key) or []
        rust_rule(_workspaces = workspaces, **kwargs)

    return rust_common_impl
