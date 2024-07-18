// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test creates multiple coin types in the same module, and show that deny actions on one type does not affect
// the other type.

//# init --accounts A --addresses test=0x0

//# publish --sender A
#[allow(deprecated_usage)]
module test::first_coin {
    use sui::coin;

    public struct FIRST_COIN has drop {}

    fun init(otw: FIRST_COIN, ctx: &mut TxContext) {
        let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency(
            otw,
            9,
            b"RC",
            b"REGULATED_COIN",
            b"A new regulated coin",
            option::none(),
            ctx
        );
        let coin = coin::mint(&mut treasury_cap, 10000, ctx);
        transfer::public_transfer(coin, tx_context::sender(ctx));
        transfer::public_transfer(deny_cap, tx_context::sender(ctx));
        transfer::public_freeze_object(treasury_cap);
        transfer::public_freeze_object(metadata);
    }
}

#[allow(deprecated_usage)]
module test::second_coin {
    use sui::coin;

    public struct SECOND_COIN has drop {}

    fun init(otw: SECOND_COIN, ctx: &mut TxContext) {
        let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency(
            otw,
            9,
            b"RC",
            b"REGULATED_COIN",
            b"A new regulated coin",
            option::none(),
            ctx
        );
        let coin = coin::mint(&mut treasury_cap, 10000, ctx);
        transfer::public_transfer(coin, tx_context::sender(ctx));
        transfer::public_transfer(deny_cap, tx_context::sender(ctx));
        transfer::public_freeze_object(treasury_cap);
        transfer::public_freeze_object(metadata);
    }
}

//# view-object 1,0

//# view-object 1,1

//# view-object 1,2

//# view-object 1,3

//# view-object 1,4

//# view-object 1,5

//# view-object 1,6

//# view-object 1,7

//# view-object 1,8

//# view-object 1,9

//# view-object 1,10

// Deny account A for FIRST_COIN.
//# run sui::coin::deny_list_add --args object(0x403) object(1,5) @A --type-args test::first_coin::FIRST_COIN --sender A

// Sending away first coin from A should fail.
//# transfer-object 1,1 --sender A --recipient A

// Sending away second coin from A should not be affected, and hence will succeed.
//# transfer-object 1,2 --sender A --recipient A
