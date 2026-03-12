// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests matching on a struct containing enum fields
module 0x42::m;

public enum Inner has drop {
    X,
    Y,
}

public struct S has drop {
    val: Inner,
    num: u64,
}

fun test(s: S): u64 {
    match (s) {
        S { val: Inner::X, num } => num,
        S { val: Inner::Y, num: _ } => 0,
    }
}
