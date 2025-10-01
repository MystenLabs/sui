// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module regulated::regulated;

use sui::coin_registry;

public struct REGULATED() has drop;

fun init(witness: REGULATED, ctx: &mut TxContext) {
    let (mut init, mut treasury_cap) = coin_registry::new_currency_with_otw(
        witness,
        2,
        b"REGULATED".to_string(),
        b"Regulated".to_string(),
        b"A fake regulated coin for test purposes".to_string(),
        b"https://example.com/regulated.png".to_string(),
        ctx,
    );

    let coin = treasury_cap.mint(1_000_000_000, ctx);
    init.make_supply_fixed(treasury_cap);

    let deny_cap = init.make_regulated(true, ctx);

    let metadata_cap = init.finalize(ctx);
    transfer::public_transfer(coin, @0x0);
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_transfer(metadata_cap, @0x0);
}
