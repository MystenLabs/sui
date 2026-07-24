// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Same-named constants from different modules are keyed independently, in constant
// definitions and in function bodies

module 0x42::a {

public(package) const MAX: u64 = 10;

}

module 0x42::b {

public(package) const MAX: u64 = 20;

}

module 0x42::c {

use 0x42::a;
use 0x42::b;

const BOTH: u64 = a::MAX + b::MAX;

public fun sum(): u64 { a::MAX + b::MAX }

public fun both(): u64 { BOTH }

}
