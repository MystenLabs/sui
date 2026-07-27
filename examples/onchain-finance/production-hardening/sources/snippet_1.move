// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#snippet_1
public fun execute_spend<T>(
    config: &AdminConfig,
    mandate: &mut SpendingMandate,
    payment: Coin<T>,
    recipient: address,
    clock: &Clock,
    ctx: &TxContext,
) {
    admin_config::assert_not_paused(config);
    // ... rest of spend logic
}
// docs::/snippet_1
