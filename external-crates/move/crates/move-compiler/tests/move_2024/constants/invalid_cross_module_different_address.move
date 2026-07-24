// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// 'public(package)' constants can only be accessed from the same address and package

module 0x42::a {

public(package) const MAX: u64 = 100;

}

module 0x43::b {

use 0x42::a;

const D: u64 = a::MAX;

public fun max(): u64 { a::MAX }

public fun d(): u64 { D }

}
