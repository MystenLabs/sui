// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module legacy::legacy;

use sui::coin;
use sui::url;

public struct LEGACY() has drop;

// Module initializer is called once on module publish. A treasury
// cap is sent to the publisher, who then controls minting and burning.
#[allow(deprecated_usage)]
fun init(witness: LEGACY, ctx: &mut TxContext) {
    let (mut treasury_cap, metadata) = coin::create_currency(
        witness,
        2,
        b"LEGACY",
        b"Legacy",
        b"A fake legacy coin for test purposes",
        option::some(url::new_unsafe_from_bytes(b"https://example.com/legacy.png")),
        ctx,
    );

    let coin = treasury_cap.mint(1_000_000_000, ctx);

    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(treasury_cap, ctx.sender());
    transfer::public_transfer(metadata, ctx.sender());
}
