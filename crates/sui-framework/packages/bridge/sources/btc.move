// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::btc {
    use std::option;
    use sui::math::pow;
    use sui::coin;
    use sui::coin::TreasuryCap;
    use sui::transfer;
    use sui::tx_context::TxContext;

    public struct BTC has drop {}

    const DECIMAL: u8 = 8;
    /// Multiplier of the token, it must be 10^DECIMAL
    const MULTIPLIER: u64 = 100_000_000;
    const EDecimalMultiplierMismatch: u64 = 0;

    public(package) fun create(ctx: &mut TxContext): TreasuryCap<BTC> {
        assert!(MULTIPLIER == pow(10, DECIMAL), EDecimalMultiplierMismatch);
        let (treasury_cap, metadata) = coin::create_currency(
            BTC {},
            DECIMAL,
            b"BTC",
            b"Bitcoin",
            b"Bridged Bitcoin token",
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
