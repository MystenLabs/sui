// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module coin_metadata::test {
    use std::option;
    use sui::coin;
    use sui::transfer;
    use sui::url;
    use sui::tx_context::TxContext;

    struct TEST has drop {}

    fun init(witness: TEST, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency<TEST>(
            witness, 
            2,
            b"TEST",
            b"Test Coin",
            b"Test coin metadata",
            option::some(url::new_unsafe_from_bytes(b"http://sui.io")),
            ctx
        );
        transfer::share_object(metadata);
        transfer::share_object(treasury_cap)
    }
}
