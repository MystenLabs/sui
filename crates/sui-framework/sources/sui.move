// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Coin<SUI> is the token used to pay for gas in Sui.
/// It has 9 decimals, and the smallest unit (10^-9) is called "mist".
module sui::sui {
    use std::option;
    use sui::tx_context::{Self, TxContext};
    use sui::balance::{Self, Balance};
    use sui::transfer;
    use sui::coin;

    friend sui::genesis;

    const EAlreadyMinted: u64 = 0;

    /// The amount of Mist per Sui token based on the the fact that mist is
    /// 10^-9 of a Sui token
    const MIST_PER_SUI: u64 = 1_000_000_000;

    /// The total supply of Sui denominated in whole Sui tokens (10 Billion)
    const TOTAL_SUPPLY_SUI: u64 = 10_000_000_000;

    /// The total supply of Sui denominated in Mist (10 Billion * 10^9)
    const TOTAL_SUPPLY_MIST: u64 = 10_000_000_000_000_000_000;

    /// Name of the coin
    struct SUI has drop {}

    /// Register the `SUI` Coin to acquire its `Supply`.
    /// This should be called only once during genesis creation.
    public(friend) fun new(ctx: &mut TxContext): Balance<SUI> {
        assert!(tx_context::epoch(ctx) == 0, EAlreadyMinted);

        let (treasury, metadata) = coin::create_currency(
            SUI {}, 
            9,
            b"SUI",
            b"Sui",
            // TODO: add appropriate description and logo url
            b"",
            option::none(),
            ctx
        );
        transfer::freeze_object(metadata);
        let supply = coin::treasury_into_supply(treasury);
        let total_sui = balance::increase_supply(&mut supply, TOTAL_SUPPLY_MIST);
        balance::destroy_supply(supply);
        total_sui
    }

    public entry fun transfer(c: coin::Coin<SUI>, recipient: address) {
        transfer::transfer(c, recipient)
    }
}
