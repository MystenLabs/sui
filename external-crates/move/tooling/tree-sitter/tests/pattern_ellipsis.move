// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    public enum X has drop {
        A { x: u64 },
        B { x: u64, y: u64 },
        C(u64, bool, bool),
    }

    public enum Y {
        A(X),
        B(u64, X),
        C { x: u64, y: X, z: u64},
    }

    public fun f(x: X): u64 {
        match (x) {
            X::A { .. } => 0,
            X::B { x, .. } => x,
            X::C(1, ..) => 1,
            X::C(1, .., true) => 2,
            X::C(.., true) => 1,
            X::C(..) => 1,
        }
    }

    public fun g(x: Y): u64 {
        match (x) {
            Y::A(X::A { .. }) => 0,
            Y::A(X::B { x, .. }) => x,
            Y::A(X::C(1, ..)) => 1,
            Y::A(X::C(1, .., true)) => 2,
            Y::A(X::C(.., true)) => 1,
            Y::A(X::C(..)) => 1,
            Y::B(.., X::A { .. }) => 0,
            Y::B(.., X::B { x, .. }) => x,
            Y::B(.., X::C(1, ..)) => 1,
            Y::B(.., X::C(1, .., true)) => 2,
            Y::B(.., X::C(.., true)) => 1,
            Y::B(_, X::C(..)) => 1,
            Y::C { x, .., y: _y} => x,
        }
    }

    public fun h(x: Y): u64 {
        match (x) {
            Y::A(_) => 0,
            // .. is zero or more!
            Y::B(_, _, ..) => 0,
            Y::C { x, y: _y, z, ..} => x + z,
        }
    }
}
