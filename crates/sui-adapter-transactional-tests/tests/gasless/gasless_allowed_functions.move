// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests each whitelisted gasless function:
// - balance::send_funds
// - balance::redeem_funds
// - funds_accumulator::withdrawal_split

//# init --addresses test=0x0 --accounts A B --enable-gasless --enable-accumulators

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
// Fund A
//> 0: sui::coin::mint<test::usdc::USDC>(Input(1), Input(0));
//> 1: sui::coin::into_balance<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(2));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

// redeem_funds: withdraw and send back to self
//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(1000) @A
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

// send_funds: send to another address
//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(2000) @B
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# view-funds sui::balance::Balance<test::usdc::USDC> B

// withdrawal_split: split 3000 to A, remainder (2000) to B
//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(5000) 3000u256 @A @B
//> 0: sui::funds_accumulator::withdrawal_split<sui::balance::Balance<test::usdc::USDC>>(Input(0), Input(1));
//> 1: sui::balance::redeem_funds<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(2));
//> 3: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 4: sui::balance::send_funds<test::usdc::USDC>(Result(3), Input(3));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# view-funds sui::balance::Balance<test::usdc::USDC> B
