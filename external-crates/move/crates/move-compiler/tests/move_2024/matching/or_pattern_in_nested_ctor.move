// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests or-patterns nested inside constructor patterns
module 0x42::m;

public enum Outer has drop {
    Wrap(Inner),
    Empty,
}

public enum Inner has drop {
    A,
    B,
    C,
}

fun test(o: Outer): u64 {
    match (o) {
        Outer::Wrap(Inner::A | Inner::B) => 1,
        Outer::Wrap(Inner::C) => 2,
        Outer::Empty => 3,
    }
}
