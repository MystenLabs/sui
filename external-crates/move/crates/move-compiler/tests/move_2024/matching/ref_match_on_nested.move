// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests reference matching with nested patterns
module 0x42::m;

public enum Outer has drop {
    A(Inner),
    B,
}

public enum Inner has drop {
    X(u64),
    Y,
}

fun test(o: &Outer): u64 {
    match (o) {
        Outer::A(Inner::X(n)) => *n,
        Outer::A(Inner::Y) => 0,
        Outer::B => 42,
    }
}
