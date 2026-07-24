// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Cross-module constants in match patterns

module 0x42::a {

public(package) const ONE: u64 = 1;
public(package) const TWO: u64 = 2;

}

module 0x42::b {

use 0x42::a;

public fun classify(x: u64): u64 {
    match (x) {
        a::ONE => 100,
        a::TWO => 200,
        n if (*n < a::ONE) => 1,
        _ => 0,
    }
}
}
