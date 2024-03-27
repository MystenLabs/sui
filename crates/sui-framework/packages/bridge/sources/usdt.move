// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::usdt {
    use sui::math::pow;
    use sui::coin::{Self, TreasuryCap};

    public struct USDT has drop {}

    const DECIMAL: u8 = 6;
    /// Multiplier of the token, it must be 10^DECIMAL
    const MULTIPLIER: u64 = 1_000_000;
    const EDecimalMultiplierMismatch: u64 = 0;

    public(package) fun create(ctx: &mut TxContext): TreasuryCap<USDT> {
        assert!(MULTIPLIER == pow(10, DECIMAL), EDecimalMultiplierMismatch);
        let (treasury_cap, metadata) = coin::create_currency(
            USDT {},
            DECIMAL,
            b"USDT",
            b"Tether",
            b"Bridged Tether token",
            option::none(),
            ctx
        );
        transfer::public_freeze_object(metadata);
        treasury_cap
    }

    public fun decimal(): u8 { DECIMAL }
    public fun multiplier(): u64 { MULTIPLIER }
}
