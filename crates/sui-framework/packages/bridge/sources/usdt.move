// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::usdt {
    use std::option;
   use sui::math::pow;
    use sui::coin;
    use sui::coin::TreasuryCap;
    use sui::transfer;
    use sui::tx_context::TxContext;

    friend bridge::treasury;

    struct USDT has drop {}

    const DECIMAL: u8 = 6;

    public(friend) fun create(ctx: &mut TxContext): TreasuryCap<USDT> {
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

    public fun decimal(): u8 {
        DECIMAL
    }

    public fun multiplier(): u64 {
        pow(10, DECIMAL)
    }
}
