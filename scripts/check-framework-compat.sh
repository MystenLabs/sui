#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Check whether the version of framework in the repo is compatible
# with the version on chain, as reported by the currently active
# environment, using the binary in environment variable $SUI.

set -e

SUI=${SUI:-sui}
REPO=$(git rev-parse --show-toplevel)

for PACKAGE in "$REPO"/crates/sui-framework/packages/*; do
    $SUI client verify-source "$PACKAGE"
done

