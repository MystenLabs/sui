// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A private constant defined from a cross-module constant is still private; a
// 'public(package)' constant may be defined from a same-module private one

module 0x42::a {

const P: u64 = 1;
public(package) const A: u64 = 10;
public(package) const Q: u64 = P + 1;

}

module 0x42::b {

use 0x42::a;

const B: u64 = a::A + 1;

public fun b(): u64 { B }
public fun q(): u64 { a::Q }

}

module 0x42::c {

use 0x42::b;

const C: u64 = b::B + 1;

public fun c(): u64 { C }

}
