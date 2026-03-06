// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests match with only wildcard arms
module 0x42::m;

public enum E has drop {
    A(u64),
    B,
}

fun test_wildcard(e: E): u64 {
    match (e) {
        _ => 0,
    }
}

fun test_binder(e: E): u64 {
    match (e) {
        _x => 0,
    }
}
