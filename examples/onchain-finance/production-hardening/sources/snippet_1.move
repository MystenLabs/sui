// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::guarded_spend;

use sui::clock::Clock;
use sui::coin::Coin;

// Placeholder types referenced from admin_config module
public struct AdminConfig has key { id: UID, paused: bool }
public struct SpendingMandate has key { id: UID }

// docs::#guarded-spend
public fun execute_spend<T>(
    config: &AdminConfig,
    mandate: &mut SpendingMandate,
    payment: Coin<T>,
    recipient: address,
    clock: &Clock,
    ctx: &TxContext,
) {
    assert!(!config.paused, 0);
    // ... rest of spend logic
    transfer::public_transfer(payment, recipient);
}
// docs::/#guarded-spend
