# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load(
    "@fbsource//tools/build_defs:python_platform.bzl",
    "set_platform_decorator_for_python",
)
load("@prelude//:native.bzl", _native = "native")

def meta_python_test(name, **kwargs):
    # Set the platform attributes as needed for proper exec platform resolution
    kwargs = set_platform_decorator_for_python(
        set_python_constraint_overrides = True,
        **kwargs
    )

    _native.python_test(
        name = name,
        **kwargs
    )
