#!/bin/sh
# Copyright (c) The Move Contributors
# SPDX-License-Identifier: Apache-2.0

TYPE="$(echo "$1" | sed s/^--resolve-move-//)"
PACKAGE="$2"

echo "Failed to resolve $TYPE for $PACKAGE" >&2
exit 1
