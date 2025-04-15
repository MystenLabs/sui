// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//docs::#regulate
module examples::regcoin;

use sui::{coin::{Self, DenyCapV2}, deny_list::DenyList};

public struct REGCOIN has drop {}

fun init(witness: REGCOIN, ctx: &mut TxContext) {
    let (treasury, deny_cap, metadata) = coin::create_regulated_currency_v2(
        witness,
        6,
        b"REGCOIN",
        b"",
        b"",
        option::none(),
        false,
        ctx,
    );
    transfer::public_freeze_object(metadata);
    transfer::public_transfer(treasury, ctx.sender());
    transfer::public_transfer(deny_cap, ctx.sender())
}

//docs::/#regulate}
public fun add_addr_from_deny_list(
    denylist: &mut DenyList,
    denycap: &mut DenyCapV2<REGCOIN>,
    denyaddy: address,
    ctx: &mut TxContext,
) {
    coin::deny_list_v2_add(denylist, denycap, denyaddy, ctx);
}

public fun remove_addr_from_deny_list(
    denylist: &mut DenyList,
    denycap: &mut DenyCapV2<REGCOIN>,
    denyaddy: address,
    ctx: &mut TxContext,
) {
    coin::deny_list_v2_remove(denylist, denycap, denyaddy, ctx);
}
