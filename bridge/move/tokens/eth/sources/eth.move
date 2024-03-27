// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridged_eth::token {
    use std::option;

    use sui::coin;
    use sui::transfer;
    use sui::tx_context;
    use sui::tx_context::TxContext;

    friend bridge::treasury;

    struct ETH has drop {}

    const DECIMAL: u8 = 8;

    fun init(otw: ETH, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency(
            otw,
            DECIMAL,
            b"ETH",
            b"Ethereum",
            b"Bridged Ethereum token",
            option::none(),
            ctx
        );
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx))
    }
}
