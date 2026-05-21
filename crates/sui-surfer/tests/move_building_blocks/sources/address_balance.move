// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Deposits SUI into the sender's address balance (accumulator), which is the
/// precondition for address-balance gas payments and gasless transactions.
module move_building_blocks::address_balance {
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;

    public fun deposit_sui(coin: Coin<SUI>, ctx: &TxContext) {
        let balance = coin.into_balance();
        balance.send_funds(ctx.sender());
    }
}
