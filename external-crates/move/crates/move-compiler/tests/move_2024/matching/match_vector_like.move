// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: match on a type with type parameter that has copy+drop
module 0x42::m;

public enum List<T: drop> has drop {
    Cons(T, Box<List<T>>),
    Nil,
}

public struct Box<T: drop> has drop {
    val: T,
}

fun head(l: List<u64>): u64 {
    match (l) {
        List::Cons(h, _) => h,
        List::Nil => 0,
    }
}
