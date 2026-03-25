// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: matching on a variant with wrong number of fields
module 0x42::m;

public enum E {
    A(u64, bool),
    B,
}

fun test(e: E): u64 {
    match (e) {
        E::A(x) => x,
        E::B => 0,
    }
}
