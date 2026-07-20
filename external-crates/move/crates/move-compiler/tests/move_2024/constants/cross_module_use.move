// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Cross-module constant uses in function bodies compile to calls of synthesized
// `public(package)` getter functions in the defining module.

module 0x42::a {

public(package) const MAX: u64 = 100;
public(package) const GREETING: vector<u8> = b"hello";

public fun max(): u64 { MAX }

}

module 0x42::b {

use 0x42::a;

public fun limit(): u64 {
    a::MAX
}

public fun double_limit(): u64 {
    a::MAX * 2
}

public fun greeting(): vector<u8> {
    a::GREETING
}

public fun check(x: u64) {
    assert!(x < a::MAX, 0);
}
}
