// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Constants without values (failed evaluation, or removed as part of an in-module cycle)
// observed cross-module: the defining modules error, the user cascades, no ICE

module 0x42::a {

public(package) const BAD: u64 = 1 / 0;

}

module 0x42::c {

public(package) const X: u64 = Y + 1;
const Y: u64 = X + 1;

}

module 0x42::b {

use 0x42::a;
use 0x42::c;

const D: u64 = a::BAD + c::X;

public fun bad(): u64 { a::BAD }

public fun x(): u64 { c::X }

public fun d(): u64 { D }

}
