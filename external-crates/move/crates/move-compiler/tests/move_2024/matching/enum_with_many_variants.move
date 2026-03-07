// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests exhaustiveness checking with many variants
module 0x42::m;

public enum Big has drop {
    V1, V2, V3, V4, V5, V6, V7, V8, V9, V10,
}

fun test(b: Big): u64 {
    match (b) {
        Big::V1 => 1,
        Big::V2 => 2,
        Big::V3 => 3,
        Big::V4 => 4,
        Big::V5 => 5,
        Big::V6 | Big::V7 | Big::V8 | Big::V9 | Big::V10 => 0,
    }
}
