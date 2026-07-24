// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Same-named constants from different modules as match arms and in or-patterns: no false
// unreachable arms, each compares its own module's value

module 0x42::a {

public(package) const MAX: u64 = 10;

}

module 0x42::b {

public(package) const MAX: u64 = 20;

}

module 0x42::c {

use 0x42::a;
use 0x42::b;

public fun which(x: u64): u64 {
    match (x) {
        a::MAX => 1,
        b::MAX => 2,
        _ => 0,
    }
}

public fun either(x: u64): bool {
    match (x) {
        a::MAX | b::MAX => true,
        _ => false,
    }
}

}
