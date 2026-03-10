// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests matching on address type
module 0x42::m;

fun test(a: address): u64 {
    match (a) {
        @0x0 => 1,
        @0x1 => 2,
        _ => 3,
    }
}
