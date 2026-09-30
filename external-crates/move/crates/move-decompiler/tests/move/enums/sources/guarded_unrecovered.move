// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Decline paths of the guard-recovery discriminator: a guard on a fieldless variant (no
// re-unpack exists to discriminate on) and a source-level `if` heading an arm body (no
// same-variant re-unpack in its branches). Neither may split into a guarded arm.
module enums::guarded_unrecovered;

public enum Order has copy, drop {
    Market { qty: u64 },
    Cancel,
}

public fun fieldless_guard(order: Order, flag: bool): u64 {
    match (order) {
        Order::Cancel if (flag) => 1,
        _ => 0,
    }
}

public fun if_in_arm(order: Order): u64 {
    match (order) {
        Order::Market { qty } => if (qty > 100) { qty * 2 } else { qty },
        _ => 0,
    }
}
