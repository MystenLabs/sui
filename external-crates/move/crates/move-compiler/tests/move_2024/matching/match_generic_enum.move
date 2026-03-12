// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests matching on a generic enum
module 0x42::m;

public enum Result<T, E> has drop {
    Ok(T),
    Err(E),
}

fun test(r: Result<u64, bool>): u64 {
    match (r) {
        Result::Ok(n) => n,
        Result::Err(true) => 1,
        Result::Err(false) => 0,
    }
}
