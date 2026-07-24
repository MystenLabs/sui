// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Same-named constants in different modules fold independently

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

public fun both(): u64 { BOTH }

}
