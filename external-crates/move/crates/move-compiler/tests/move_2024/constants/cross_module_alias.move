// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Cross-module constants through a member 'use' alias

module 0x42::a {

public(package) const MAX: u64 = 100;

}

module 0x42::b {

use 0x42::a::MAX;

const D: u64 = MAX + 1;

public fun max(): u64 { MAX }

public fun d(): u64 { D }

}
