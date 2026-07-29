// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// 'public(package)' is the only visibility valid on constants

module 0x42::m {

public const A: u64 = 0;

public(friend) const B: u64 = 1;

public(package) const C: u64 = 2;

public fun values(): (u64, u64, u64) { (A, B, C) }

}
