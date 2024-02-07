// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::btc {
    use std::option;

    use sui::coin;
    use sui::coin::TreasuryCap;
    use sui::transfer;
    use sui::tx_context::TxContext;

    friend bridge::treasury;

    struct BTC has drop {}

    const DECIMAL: u8 = 8;

    public(friend) fun create(ctx: &mut TxContext): TreasuryCap<BTC> {
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
}
