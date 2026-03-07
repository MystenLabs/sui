// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests enums mixing unit variants with tuple/named variants
module 0x42::m;

public enum Mixed has drop {
    Unit,
    Tuple(u64, bool),
    Named { x: u64 },
}

fun test(m: Mixed): u64 {
    match (m) {
        Mixed::Unit => 0,
        Mixed::Tuple(n, true) => n,
        Mixed::Tuple(n, false) => n + 1,
        Mixed::Named { x } => x,
    }
}
