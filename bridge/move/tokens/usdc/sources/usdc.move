// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridged_usdc::usdc {
    use std::option;

    use sui::coin;
    use sui::transfer;
    use sui::tx_context;
    use sui::tx_context::TxContext;

    struct USDC has drop {}

    const DECIMAL: u8 = 6;

    fun init(otw: USDC, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency(
            otw,
            DECIMAL,
            b"USDC",
            b"USD Coin",
            b"Bridged USD Coin token",
            option::none(),
            ctx
        );
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
    }
}
