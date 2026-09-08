// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module enums::guarded;

public enum Order has copy, drop {
    Market { qty: u64 },
    Limit { qty: u64, price: u64 },
    Cancel,
}

const BAD_ORDER: u64 = 1;

// One guarded arm, same-variant fallthrough arm.
public fun market_fee(order: Order): u64 {
    match (order) {
        Order::Market { qty } if (*qty > 100) => qty * 2,
        Order::Market { qty } => qty,
        _ => 0,
    }
}

// Guard falling through to a wildcard arm of a different variant.
public fun is_large(order: Order): bool {
    match (order) {
        Order::Limit { qty, price } if (*qty * *price > 1000) => true,
        _ => false,
    }
}

// Guards on a by-reference match.
public fun priority(order: &Order): u64 {
    match (order) {
        Order::Market { qty } if (*qty > 0) => 3,
        Order::Limit { qty, price: _ } if (*qty > 0) => 2,
        Order::Cancel => 1,
        _ => abort BAD_ORDER,
    }
}

// Two guards on the same variant plus an unguarded arm.
public fun tiered(order: Order): u64 {
    match (order) {
        Order::Limit { qty, price: _ } if (*qty > 100) => 3,
        Order::Limit { qty, price: _ } if (*qty > 10) => 2,
        Order::Limit { .. } => 1,
        _ => 0,
    }
}
