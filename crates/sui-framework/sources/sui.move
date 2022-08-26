// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Coin<SUI> is the token used to pay for gas in Sui
module sui::sui {
    use sui::balance::{Self, Supply};
    use sui::coin;
    use sui::transfer;

    friend sui::genesis;

    /// Name of the coin
    struct SUI has drop {}

    /// Register the token to acquire its `TreasuryCap`.
    /// This should be called only once during genesis creation.
    public(friend) fun new(): Supply<SUI> {
        balance::create_supply(SUI {})
    }

    public entry fun transfer(c: coin::Coin<SUI>, recipient: address) {
        transfer::transfer(c, recipient)
    }
}
