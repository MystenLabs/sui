// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests matching on structs with multiple fields and mixing named/positional
module 0x42::m;

public struct Pair has drop {
    first: u64,
    second: bool,
}

public enum E has drop {
    Wrap(Pair),
    None,
}

fun test_struct_in_enum(e: E): u64 {
    match (e) {
        E::Wrap(Pair { first: 0, second: true }) => 1,
        E::Wrap(Pair { first, second: _ }) => first,
        E::None => 99,
    }
}

fun test_struct_direct(p: Pair): u64 {
    match (p) {
        Pair { first: 0, second: true } => 1,
        Pair { first: 0, second: false } => 2,
        Pair { first, .. } => first,
    }
}
