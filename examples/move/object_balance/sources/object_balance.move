// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module object_balance::object_balance;

use sui::balance::Balance;

public struct Vault has key {
    id: UID,
}

public fun new(ctx: &mut TxContext) {
    let vault = Vault {
        id: object::new(ctx),
    };
    transfer::transfer(vault, ctx.sender());
}

public fun withdraw_funds<T>(vault: &mut Vault, amount: u64): Balance<T> {
    sui::balance::redeem_funds(
        sui::balance::withdraw_funds_from_object(
            &mut vault.id,
            amount,
        ),
    )
}
