# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//:native.bzl", _native = "native")

# Public symbols in this file become globals everywhere except `bzl` files in prelude.
# Additionally, members of `native` struct also become globals in `BUCK` files.
native = _native

# This is a test to get CI to notice me
