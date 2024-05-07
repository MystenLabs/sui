// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::eth {
    use sui::coin;

    public struct ETH has drop {}

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
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
    }
}
