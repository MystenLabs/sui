// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//docs::#regulate
module examples::regcoin_new;

use sui::coin::{Self, DenyCapV2};
use sui::coin_registry;
use sui::deny_list::DenyList;

public struct REGCOIN_NEW has drop {}

fun init(witness: REGCOIN_NEW, ctx: &mut TxContext) {
    let (mut currency, treasury_cap) = coin_registry::new_currency_with_otw(
        witness,
        6, // Decimals
        b"REGCOIN".to_string(), // Symbol
        b"Regulated Coin".to_string(), // Name
        b"Currency with DenyList Support".to_string(), // Description
        b"https://example.com/regcoin.png".to_string(), // Icon URL
        ctx,
    );

    // Claim `DenyCapV2` and mark currency as regulated.
    let deny_cap = currency.make_regulated(true, ctx);
    let metadata_cap = currency.finalize(ctx);
    let sender = ctx.sender();

    transfer::public_transfer(treasury_cap, sender);
    transfer::public_transfer(metadata_cap, sender);
    transfer::public_transfer(deny_cap, sender)
}

//docs::/#regulate}
public fun add_addr_from_deny_list(
    denylist: &mut DenyList,
    denycap: &mut DenyCapV2<REGCOIN_NEW>,
    denyaddy: address,
    ctx: &mut TxContext,
) {
    coin::deny_list_v2_add(denylist, denycap, denyaddy, ctx);
}

public fun remove_addr_from_deny_list(
    denylist: &mut DenyList,
    denycap: &mut DenyCapV2<REGCOIN_NEW>,
    denyaddy: address,
    ctx: &mut TxContext,
) {
    coin::deny_list_v2_remove(denylist, denycap, denyaddy, ctx);
}
