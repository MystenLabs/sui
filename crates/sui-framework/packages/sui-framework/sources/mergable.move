// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::mergable;

/// A commutative sum type. It is represented as a u128, but can only
/// be created from a u64. This ensures that overflow is impossible unless
/// 2^64 Sums are created and added together - a practical impossibility.
public struct Sum has store {
    value: u128,
}

public fun make_sum(value: u64): Sum {
    Sum { value: value as u128 }
}