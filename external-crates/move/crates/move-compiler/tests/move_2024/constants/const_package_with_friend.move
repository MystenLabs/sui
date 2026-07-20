// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// 'public(package)' constants cannot be mixed with 'friend' declarations

module 0x42::a {

friend 0x42::b;

public(package) const MAX: u64 = 100;

public fun max(): u64 { MAX }

}

module 0x42::b {}
