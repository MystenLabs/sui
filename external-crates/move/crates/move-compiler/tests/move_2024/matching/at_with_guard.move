// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests at-pattern combined with guard expression
module 0x42::m;

public enum E has drop, copy {
    A(u64),
    B,
}

fun test(e: &E): u64 {
    match (e) {
        x @ E::A(n) if (*n > 10) => *n,
        _ => 0,
    }
}
