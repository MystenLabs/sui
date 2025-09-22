// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module burn_only::burn;

use sui::coin_registry;

public struct BURN() has drop;

fun init(witness: BURN, ctx: &mut TxContext) {
    let (mut init, mut treasury_cap) = coin_registry::new_currency_with_otw(
        witness,
        2,
        b"BURN".to_string(),
        b"Burn".to_string(),
        b"A fake burn-only coin for test purposes".to_string(),
        b"https://example.com/fake.png".to_string(),
        ctx,
    );

    let coin = treasury_cap.mint(1_000_000_000, ctx);
    init.make_supply_burn_only(treasury_cap);

    let metadata_cap = init.finalize(ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(metadata_cap, @0x0);
}
