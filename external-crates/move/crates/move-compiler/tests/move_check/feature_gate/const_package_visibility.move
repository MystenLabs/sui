// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// 'public(package)' constants require the cross-module constants feature

module 0x42::m {
    public(package) const MAX: u64 = 100;

    public fun max(): u64 { MAX }
}
