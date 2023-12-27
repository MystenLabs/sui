// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::eth {
    use std::option;

    use sui::coin;
    use sui::transfer;
    use sui::tx_context::TxContext;

    friend bridge::treasury;

    struct ETH has drop {}

    fun init(witness: ETH, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency(
            witness,
            // ETC DP limited to 8 on Sui
            8,
            b"ETH",
            b"Ethereum",
            b"Bridged Ethereum token",
            option::none(),
            ctx
        );
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, @0xf82999a527fe455c8379a9132fa7f8a0e024575810bcef69e26d4d6dc2830647);
    }
}
