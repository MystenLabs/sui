// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests gasless multi-recipient transfer using withdrawal_split.

//# init --addresses test=0x0 --accounts A B C --enable-gasless --enable-accumulators

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

//# programmable --sender A --inputs 1000 object(1,1) @A
// Mint 1000 USDC and fund A
//> 0: sui::coin::mint<test::usdc::USDC>(Input(1), Input(0));
//> 1: sui::coin::into_balance<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(2));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(1000) 400u256 @B @C
// Gasless: A splits withdrawal - 400 to B, remainder (600) to C
//> 0: sui::funds_accumulator::withdrawal_split<sui::balance::Balance<test::usdc::USDC>>(Input(0), Input(1));
//> 1: sui::balance::redeem_funds<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(2));
//> 3: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 4: sui::balance::send_funds<test::usdc::USDC>(Result(3), Input(3));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# view-funds sui::balance::Balance<test::usdc::USDC> B

//# view-funds sui::balance::Balance<test::usdc::USDC> C
