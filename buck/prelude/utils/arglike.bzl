# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

# Command-line argument-like. For example, a string, or an artifact.
# Precise list is defined in `ValueAsCommandLineLike::as_command_line`.
# Defining as Any, but can be defined as union type,
# but that might be expensive to check at runtime.
# In the future we will have compiler-time only types,
# and this type could be refined.
ArgLike = typing.Any
