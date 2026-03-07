// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests interaction of constant patterns with at-patterns and guards
module 0x42::m;

const C: u64 = 42;

public enum Option<T> has drop {
    Some(T),
    None,
}

fun test(o: Option<u64>): u64 {
    match (o) {
        Option::Some(x @ C) => x,
        Option::Some(_) => 0,
        Option::None => 99,
    }
}
