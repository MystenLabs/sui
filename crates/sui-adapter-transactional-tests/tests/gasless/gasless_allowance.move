// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Gasless spends through an allowance: B spends A's USDC via a rate-limited allowance. The
// allowance is written without storage cost, rebate, or non-refundable fee.

//# init --addresses test=0x0 --accounts A B C --enable-gasless --simulator

//# publish --sender A
#[allow(deprecated_usage)]
module test::usdc {
    use sui::coin;

    public struct USDC has drop {}

    fun init(otw: USDC, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency(
            otw, 6, b"USDC", b"USD Coin", b"", option::none(), ctx,
        );
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, ctx.sender());
    }
}

//# gasless-allow-token test::usdc::USDC

//# programmable --sender A --inputs 10000 object(1,1) @A
// Mint 10000 USDC to A's address balance.
//> 0: sui::coin::mint<test::usdc::USDC>(Input(1), Input(0));
//> 1: sui::coin::into_balance<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(2));

//# create-checkpoint

//# programmable --sender A --inputs b"b" @B vector[5000u256] vector[] vector[] 86400000 1000u256
// A issues B an allowance: 5000 lifetime cap, 1000 per day.
//> 0: sui::allowance::periodic_rate_limit(Input(5), Input(6));
//> 1: std::option::some<sui::allowance::RateLimit>(Result(0));
//> 2: sui::allowance::new<sui::balance::Balance<test::usdc::USDC>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(1));

//# view-object 5,0

//# programmable --sender B --address-balance-gas --gas-price 0 --gas-budget 0 --inputs allowance_withdraw<sui::balance::Balance<test::usdc::USDC>>(300,@A,object(5,0)) mutshared(5,0) immshared(6) @C
// Gasless spend. The first rate-limited charge sets the window anchor, growing the allowance.
//> 0: sui::allowance::balance_spend<test::usdc::USDC>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(3));

//# view-object 5,0

//# programmable --sender B --address-balance-gas --gas-price 0 --gas-budget 0 --inputs allowance_withdraw<sui::balance::Balance<test::usdc::USDC>>(500,@A,object(5,0)) mutshared(5,0) immshared(6) @C
// Second gasless spend: the allowance no longer grows.
//> 0: sui::allowance::balance_spend<test::usdc::USDC>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(3));

//# view-object 5,0

//# programmable --sender B --address-balance-gas --gas-price 0 --gas-budget 0 --inputs allowance_withdraw<sui::balance::Balance<test::usdc::USDC>>(201,@A,object(5,0)) mutshared(5,0) immshared(6) @C
// Exceeds the daily limit: aborts, and is still free.
//> 0: sui::allowance::balance_spend<test::usdc::USDC>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(3));

//# view-object 5,0

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# view-funds sui::balance::Balance<test::usdc::USDC> C
