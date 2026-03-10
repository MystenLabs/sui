// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests constant patterns nested inside variant patterns
module 0x42::m;

const MY_CONST: u64 = 42;

public enum Option<T> has drop {
    Some(T),
    None,
}

fun test(o: Option<u64>): u64 {
    match (o) {
        Option::Some(MY_CONST) => 1,
        Option::Some(_) => 2,
        Option::None => 3,
    }
}
