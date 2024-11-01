// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test verifies when sending two objects of different coin types in the same transaction,
// if one is denied but not the other, the transaction check should still fail.
// More importantly, if the second type is denied but not the first, the fact that
// the first type doesn't even have a denylist entry should not matter.

//# init --accounts A --addresses test=0x0

//# publish --sender A
module test::regulated_coin1 {
    use sui::coin;

    public struct REGULATED_COIN1 has drop {}

    fun init(otw: REGULATED_COIN1, ctx: &mut TxContext) {
        let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
            otw,
            9,
            b"RC",
            b"REGULATED_COIN",
            b"A new regulated coin",
            option::none(),
            false,
            ctx
        );
        let coin = coin::mint(&mut treasury_cap, 10000, ctx);
        transfer::public_transfer(coin, tx_context::sender(ctx));
        transfer::public_transfer(deny_cap, tx_context::sender(ctx));
        transfer::public_freeze_object(treasury_cap);
        transfer::public_freeze_object(metadata);
    }
}

module test::regulated_coin2 {
    use sui::coin;

    public struct REGULATED_COIN2 has drop {}

    fun init(otw: REGULATED_COIN2, ctx: &mut TxContext) {
        let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
            otw,
            9,
            b"RC",
            b"REGULATED_COIN",
            b"A new regulated coin",
            option::none(),
            false,
            ctx
        );
        let coin = coin::mint(&mut treasury_cap, 10000, ctx);
        transfer::public_transfer(coin, tx_context::sender(ctx));
        transfer::public_transfer(deny_cap, tx_context::sender(ctx));
        transfer::public_freeze_object(treasury_cap);
        transfer::public_freeze_object(metadata);
    }
}

// Coin1
//# view-object 1,1

// Coin2
//# view-object 1,2

// Deny account A for coin2.
//# run sui::coin::deny_list_v2_add --args object(0x403) object(1,6) @A --type-args test::regulated_coin2::REGULATED_COIN2 --sender A

//# programmable --sender A --inputs object(1,1) object(1,2) @A
//> TransferObjects([Input(0), Input(1)], Input(2))
