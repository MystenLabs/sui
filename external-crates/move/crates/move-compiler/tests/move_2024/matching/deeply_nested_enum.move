// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests deeply nested enum patterns (3+ levels deep)
module 0x42::m;

public enum A has drop {
    Wrap(B),
    End,
}

public enum B has drop {
    Wrap(C),
    End,
}

public enum C has drop {
    Val(u64),
    End,
}

fun test(a: A): u64 {
    match (a) {
        A::Wrap(B::Wrap(C::Val(n))) => n,
        A::Wrap(B::Wrap(C::End)) => 1,
        A::Wrap(B::End) => 2,
        A::End => 3,
    }
}
