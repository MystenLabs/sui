// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::guarded_spend;

use example::admin_config::AdminConfig;
use sui::balance::{Self, Balance};
use sui::clock::Clock;

#[error(code = 0)]
const EMandateExpired: vector<u8> = b"Spending mandate has expired";

public struct SpendingMandate<phantom T> has key {
    id: UID,
    expires_at_ms: u64,
    balance: Balance<T>,
}

// docs::#guarded-spend
public fun execute_spend<T>(
    config: &AdminConfig,
    mandate: &mut SpendingMandate<T>,
    amount: u64,
    recipient: address,
    clock: &Clock,
) {
    example::admin_config::assert_not_paused(config);
    assert!(clock.timestamp_ms() < mandate.expires_at_ms, EMandateExpired);
    balance::send_funds(mandate.balance.split(amount), recipient);
}
// docs::/#guarded-spend
