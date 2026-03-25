// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: or-pattern provides exhaustive coverage for inner nested enum
module 0x42::m;

public enum Outer has drop {
    A(Inner),
    B,
}

public enum Inner has drop {
    X,
    Y,
    Z,
}

fun test(o: Outer): u64 {
    match (o) {
        Outer::A(Inner::X | Inner::Y | Inner::Z) => 1,
        Outer::B => 0,
    }
}
