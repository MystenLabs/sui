// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A constant dependency cycle across modules is a module dependency cycle, whether through
// constant definitions or function-body uses

module 0x42::a {

use 0x42::b;

public(package) const A: u64 = b::B + 1;

}

module 0x42::b {

use 0x42::a;

public(package) const B: u64 = a::A + 1;
}

module 0x42::c {

public(package) const C: u64 = 1;

public fun uses_d(): u64 { 0x42::d::D }

}

module 0x42::d {

public(package) const D: u64 = 2;

public fun uses_c(): u64 { 0x42::c::C }
}
