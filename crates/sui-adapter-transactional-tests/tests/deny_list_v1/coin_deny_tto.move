// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test verifies the deny list also applies to receiving coin objects.

//# init --accounts A --addresses test=0x0

//# publish --sender A
#[allow(deprecated_usage)]
module test::regulated_coin {
    use sui::coin;
    use sui::coin::Coin;
    use sui::transfer::Receiving;

    public struct REGULATED_COIN has drop {}

    public struct Wallet has key {
        id: UID,
    }

    fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
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
        let wallet = Wallet {
            id: object::new(ctx),
        };
        transfer::public_transfer(coin, object::id_address(&wallet));
        transfer::transfer(wallet, tx_context::sender(ctx));

        transfer::public_transfer(deny_cap, tx_context::sender(ctx));
        transfer::public_freeze_object(treasury_cap);
        transfer::public_freeze_object(metadata);
    }

    public fun receive_coin(wallet: &mut Wallet, coin: Receiving<Coin<REGULATED_COIN>>, ctx: &TxContext) {
        let coin = transfer::public_receive(&mut wallet.id, coin);
        transfer::public_transfer(coin, tx_context::sender(ctx));
    }
}

//# view-object 1,0

//# view-object 1,1

//# view-object 1,2

//# view-object 1,3

//# view-object 1,4

//# view-object 1,5

//# view-object 1,6

// Deny account A.
//# run sui::coin::deny_list_add --args object(0x403) object(1,4) @A --type-args test::regulated_coin::REGULATED_COIN --sender A

// Try to receive coin in Wallet. This should now fail.
//# run test::regulated_coin::receive_coin --args object(1,0) receiving(1,2) --sender A

// Undeny account A.
//# run sui::coin::deny_list_remove --args object(0x403) object(1,4) @A --type-args test::regulated_coin::REGULATED_COIN --sender A

// Try to receive coin in Wallet. This should now succeed.
//# run test::regulated_coin::receive_coin --args object(1,0) receiving(1,2) --sender A
