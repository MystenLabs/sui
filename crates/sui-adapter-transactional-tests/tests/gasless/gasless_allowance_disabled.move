// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Without gasless_allowance_spend, gasless transactions reject shared inputs.

//# init --addresses test=0x0 --accounts A B --enable-gasless --disable-feature-flags gasless_allowance_spend

//# publish --sender A
#[allow(deprecated_usage)]
module test::usdc {
    use sui::coin::{Self, Coin};

    public struct USDC has drop {}

    fun init(otw: USDC, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency(
            otw, 6, b"USDC", b"USD Coin", b"", option::none(), ctx,
        );
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, ctx.sender());
    }

    public fun share_coin(coin: Coin<USDC>) {
        transfer::public_share_object(coin);
    }
}

//# gasless-allow-token test::usdc::USDC

//# programmable --sender A --inputs object(1,1) 1000
// Mint and share a coin.
//> 0: sui::coin::mint<test::usdc::USDC>(Input(0), Input(1));
//> test::usdc::share_coin(Result(0));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs object(3,0) @B
// Rejected: a shared coin.
//> 0: sui::coin::send_funds<test::usdc::USDC>(Input(0), Input(1));
