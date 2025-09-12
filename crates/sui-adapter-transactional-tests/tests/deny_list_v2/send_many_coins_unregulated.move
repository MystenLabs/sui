// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::coin {
    use sui::coin;

    public struct COIN has drop {}

    fun init(otw: COIN, ctx: &mut TxContext) {
        let (mut treasury_cap, metadata) = coin::create_currency(
            otw,
            9,
            b"c",
            b"COIN",
            b"A new coin",
            option::none(),
            ctx
        );
        let coin = coin::mint(&mut treasury_cap, 100000, ctx);
        transfer::public_transfer(coin, ctx.sender());
        transfer::public_freeze_object(treasury_cap);
        transfer::public_freeze_object(metadata);
    }

    public fun send_1(coin: &mut coin::Coin<COIN>, ctx: &mut TxContext) {
        use sui::transfer::public_transfer;
        public_transfer(coin.split(1, ctx), @0);
    }

    public fun send_10(coin: &mut coin::Coin<COIN>, ctx: &mut TxContext) {
        use sui::transfer::public_transfer;
        use sui::address;
        let mut i = 0u64;
        while (i < 10) {
            public_transfer(coin.split(1, ctx), address::from_u256(i as u256));
            i = i + 1;
        }
    }

    public fun send_100(coin: &mut coin::Coin<COIN>, ctx: &mut TxContext) {
        use sui::transfer::public_transfer;
        use sui::address;
        let mut i = 0u64;
        while (i < 100) {
            public_transfer(coin.split(1, ctx), address::from_u256(i as u256));
            i = i + 1;
        }
    }
}

//# view-object 1,1

//# run test::coin::send_1 --args object(1,1) --sender A

//# run test::coin::send_10 --args object(1,1) --sender A

//# run test::coin::send_100 --args object(1,1) --sender A
