// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests boolean matching with guards - verifies guard + bool saturation interaction
module 0x42::m;

fun test_bool_guard(b: bool, x: u64): u64 {
    match (b) {
        true if (x > 10) => 1,
        true => 2,
        false => 3,
    }
}

fun test_bool_all_guarded(b: bool, x: u64): u64 {
    match (b) {
        true if (x > 10) => 1,
        false if (x > 20) => 2,
        _ => 3,
    }
}
