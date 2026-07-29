// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::guarded_spend;

use example::admin_config::AdminConfig;
use sui::clock::Clock;
use sui::coin::Coin;

public struct SpendingMandate has key {
    id: UID,
}

// docs::#guarded-spend
public fun execute_spend<T>(
    config: &AdminConfig,
    _mandate: &mut SpendingMandate,
    payment: Coin<T>,
    recipient: address,
    clock: &Clock,
    _ctx: &mut TxContext,
) {
    example::admin_config::assert_not_paused(config);
    // Read the clock to satisfy the lint (real code checks expiry here).
    let _ = clock.timestamp_ms();
    // ... rest of spend logic
    transfer::public_transfer(payment, recipient);
}
// docs::/#guarded-spend
