// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module legacy_regulated::regulated;

use sui::coin;
use sui::url;

public struct REGULATED() has drop;

#[allow(deprecated_usage)]
fun init(witness: REGULATED, ctx: &mut TxContext) {
    let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
        witness,
        2,
        b"REGULATED",
        b"LegacyRegulated",
        b"A fake legacy regulated coin for test purposes",
        option::some(url::new_unsafe_from_bytes(b"https://example.com/regulated.png")),
        false,
        ctx,
    );

    let coin = treasury_cap.mint(1_000_000_000, ctx);

    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(treasury_cap, ctx.sender());
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_transfer(metadata, ctx.sender());
}
