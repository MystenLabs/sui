// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module x::a;

fun bar(x: u64): u64 {
    match (x) {
        1 => 1, // ERROR
        _ => 0,
    };
}

fun foo(x: u64): u64 {
    match (x) {
        1 | 2 | 3 => 1, // ERROR
        _ => 0,
    };
}
