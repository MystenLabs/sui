// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::usdc {
    use std::option;
    use sui::math::pow;
    use sui::coin;
    use sui::coin::TreasuryCap;
    use sui::transfer;
    use sui::tx_context::TxContext;

    public struct USDC has drop {}

    const DECIMAL: u8 = 6;
    /// Multiplier of the token, it must be 10^DECIMAL
    const MULTIPLIER: u64 = 1_000_000;
    const EDecimalMultiplierMismatch: u64 = 0;

    public(package) fun create(ctx: &mut TxContext): TreasuryCap<USDC> {
        assert!(MULTIPLIER == pow(10, DECIMAL), EDecimalMultiplierMismatch);
        let (treasury_cap, metadata) = coin::create_currency(
            USDC {},
            DECIMAL,
            b"USDC",
            b"USD Coin",
            b"Bridged USD Coin token",
            option::none(),
            ctx
        );
        transfer::public_freeze_object(metadata);
        treasury_cap
    }

    public fun decimal(): u8 {
        DECIMAL
    }

    public fun multiplier(): u64 {
        MULTIPLIER
    }
}
