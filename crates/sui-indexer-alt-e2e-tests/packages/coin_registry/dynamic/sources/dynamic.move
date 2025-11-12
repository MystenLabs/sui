// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module dynamic::dynamic;

use sui::coin_registry::{Self, CoinRegistry};

public struct Dynamic has key { id: UID }

entry fun new_currency(
    registry: &mut CoinRegistry,
    ctx: &mut TxContext,
) {
    let (mut init, mut treasury_cap) = coin_registry::new_currency<Dynamic>(
        registry,
        2,
        b"DYNAMIC".to_string(),
        b"Dynamic".to_string(),
        b"A fake dynamic coin for test purposes".to_string(),
        b"https://example.com/dynamic.png".to_string(),
        ctx,
    );

    let coin = treasury_cap.mint(1_000_000_000, ctx);
    init.make_supply_fixed(treasury_cap);

    let metadata_cap = init.finalize(ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(metadata_cap, @0x0);
}
