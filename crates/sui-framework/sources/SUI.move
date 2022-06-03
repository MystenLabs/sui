// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Coin<SUI> is the token used to pay for gas in Sui
module Sui::SUI {
    use Sui::Coin;
    use Sui::Coin::TreasuryCap;
    use Sui::TxContext::TxContext;

    friend Sui::Genesis;

    /// Name of the coin
    struct SUI has drop {}

    /// Register the token to acquire its `TreasuryCap`.
    /// This should be called only once during genesis creation.
    public(friend) fun new(ctx: &mut TxContext): TreasuryCap<SUI> {
        Coin::create_currency(SUI{}, ctx)
    }

    /// Transfer to a recipient
    public(script) fun transfer(c: Coin::Coin<SUI>, recipient: address) {
        Coin::transfer(c, recipient)
    }

}
