// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example coin with a trusted manager responsible for minting/burning (e.g., a stablecoin)
/// By convention, modules defining custom coin types use upper case names, in contrast to
/// ordinary modules, which use camel case.
module fungible_tokens::managed {
    use sui::coin::{Self, Coin, TreasuryCap};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// Name of the coin. By convention, this type has the same name as its parent module
    /// and has no fields. The full type of the coin defined by this module will be `COIN<MANAGED>`.
    struct MANAGED has drop {}

    /// Register the managed currency to acquire its `TreasuryCap`. Because
    /// this is a module initializer, it ensures the currency only gets
    /// registered once.
    fun init(ctx: &mut TxContext) {
        // Get a treasury cap for the coin and give it to the transaction sender
        let treasury_cap = coin::create_currency<MANAGED>(MANAGED{}, ctx);
        transfer::transfer(treasury_cap, tx_context::sender(ctx))
    }

    /// Manager can mint new coins
    public fun mint(treasury_cap: &mut TreasuryCap<MANAGED>, amount: u64, ctx: &mut TxContext): Coin<MANAGED> {
        coin::mint<MANAGED>(treasury_cap, amount, ctx)
    }

    /// Manager can burn coins
    public entry fun burn(treasury_cap: &mut TreasuryCap<MANAGED>, coin: Coin<MANAGED>) {
        coin::burn(treasury_cap, coin);
    }

    /// Manager can transfer the treasury capability to a new manager
    public entry fun transfer_cap(treasury_cap: TreasuryCap<MANAGED>, recipient: address) {
        coin::transfer_cap<MANAGED>(treasury_cap, recipient);
    }

    #[test_only]
    /// Wrapper of module initializer for testing
    public fun test_init(ctx: &mut TxContext) {
        init(ctx)
    }
}
