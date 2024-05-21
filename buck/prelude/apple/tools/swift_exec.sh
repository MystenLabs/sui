#!/usr/bin/env bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

set -e

if [ -n "$INSIDE_RE_WORKER" ]; then
    # Use $TMPDIR for the module cache location. This
    # will be set to a unique location for each RE action
    # which will avoid sharing modules across RE actions.
    # This is necessary as the inputs to the modules will
    # be transient and can be removed at any point, causing
    # module validation errors to fail builds.
    # https://github.com/llvm/llvm-project/blob/main/clang/lib/Driver/ToolChains/Clang.cpp#L3709
    export CLANG_MODULE_CACHE_PATH="$TMPDIR/buck-module-cache"
else
    # For local actions use a shared module cache location.
    # This should be safe to share across the other local
    # compilation actions.
    export CLANG_MODULE_CACHE_PATH="/tmp/buck-module-cache"
fi

# Apply a debug prefix map for the current directory
# to make debug info relocatable. To correctly make paths
# relocatable, we must use that path at which the action
# is run (be it locally or on RE) and this is not known
# at the time of action definition.
exec "$@" -debug-prefix-map "$PWD"/=
