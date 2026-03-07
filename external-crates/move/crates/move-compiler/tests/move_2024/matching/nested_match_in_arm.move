// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests nested match expressions (match inside match arm body)
module 0x42::m;

public enum Outer {
    A(Inner),
    B,
}

public enum Inner {
    X(u64),
    Y,
}

fun nested_match(o: Outer): u64 {
    match (o) {
        Outer::A(inner) => match (inner) {
            Inner::X(n) => n,
            Inner::Y => 0,
        },
        Outer::B => 42,
    }
}
