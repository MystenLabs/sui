// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests or-patterns nested inside struct fields
module 0x42::m;

public enum Inner has drop {
    A,
    B,
    C,
}

public struct S has drop {
    x: Inner,
    y: Inner,
}

fun test(s: S): u64 {
    match (s) {
        S { x: Inner::A | Inner::B, y: Inner::A | Inner::B } => 1,
        S { x: Inner::C, y: _ } => 2,
        S { x: _, y: Inner::C } => 3,
    }
}
