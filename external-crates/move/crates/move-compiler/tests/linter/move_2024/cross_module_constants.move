// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Cross-module constant uses produce no lint output: the generated constant functions reuse
// the constant's location, so a lint firing on them would point at the declaration

module 0x42::a {

public(package) const MAX: u64 = 100;

}

module 0x42::b {

use 0x42::a;

const DOUBLE: u64 = a::MAX * 2;

public fun max(): u64 { a::MAX }
public fun double(): u64 { DOUBLE }

}
