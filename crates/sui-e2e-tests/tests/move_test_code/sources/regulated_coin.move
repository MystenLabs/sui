// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_test_code::regulated_coin {
    use std::option;
    use sui::coin;
    use sui::coin::Coin;
    use sui::object;
    use sui::object::UID;
    use sui::transfer;
    use sui::transfer::Receiving;
    use sui::tx_context;
    use sui::tx_context::TxContext;

    struct REGULATED_COIN has drop {}

    struct Wallet has key {
        id: UID,
    }

    fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
        let (treasury_cap, deny_cap, metadata) = coin::create_regulated_currency(
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

    public fun new_wallet(ctx: &mut TxContext) {
        let wallet = Wallet {
            id: object::new(ctx),
        };
        transfer::transfer(wallet, tx_context::sender(ctx));
    }

    public fun receive_coin(wallet: &mut Wallet, coin: Receiving<Coin<REGULATED_COIN>>, ctx: &TxContext) {
        let coin = transfer::public_receive(&mut wallet.id, coin);
        transfer::public_transfer(coin, tx_context::sender(ctx));
    }
}
