// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module coin_deny_list_v2::regulated_coin {
    use std::option;
    use sui::coin::{Self, DenyCapV2};
    use sui::deny_list::DenyList;
    use sui::object::UID;
    use sui::transfer;
    use sui::tx_context;
    use sui::tx_context::TxContext;

    public struct REGULATED_COIN has drop {}

    public struct Wallet has key {
        id: UID,
    }

    fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
        let (treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
            otw,
            9,
            b"RC",
            b"REGULATED_COIN",
            b"A new regulated coin",
            option::none(),
            true,
            ctx
        );
        transfer::public_transfer(deny_cap, tx_context::sender(ctx));
        transfer::public_freeze_object(treasury_cap);
        transfer::public_freeze_object(metadata);
    }
}
