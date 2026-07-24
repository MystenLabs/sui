// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Constants defined in terms of other modules' constants are folded at compile time, across
// module boundaries, including chains through multiple modules. No getters are synthesized for
// constants only used in other constant definitions.

module 0x42::a {

public(package) const BASE: u64 = 10;

}

module 0x42::b {

use 0x42::a;

public(package) const DOUBLE: u64 = a::BASE * 2;

}

module 0x42::c {

use 0x42::a;
use 0x42::b;

const QUAD: u64 = b::DOUBLE * 2;
const MIXED: u64 = a::BASE + b::DOUBLE + QUAD;

public fun quad(): u64 { QUAD }
public fun mixed(): u64 { MIXED }
}
