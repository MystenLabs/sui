// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module fixed_supply::fixed;

use sui::coin_registry;

public struct FIXED() has drop;

fun init(witness: FIXED, ctx: &mut TxContext) {
    let (mut init, mut treasury_cap) = coin_registry::new_currency_with_otw(
        witness,
        2,
        b"FIXED".to_string(),
        b"Fixed".to_string(),
        b"A fake fixed-supply coin for test purposes".to_string(),
        b"https://example.com/fake.png".to_string(),
        ctx,
    );

    let coin = treasury_cap.mint(1_000_000_000, ctx);
    init.make_supply_fixed(treasury_cap);

    let metadata_cap = init.finalize(ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(metadata_cap, @0x0);
}
