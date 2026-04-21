// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that gasless withdrawals leaving a balance below the minimum are rejected.
// gasless_verify_remaining_balance is enabled by --enable-gasless.

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

//# gasless-allow-token test::usdc::USDC --min-transfer 1000

//# programmable --sender A --inputs 10000 object(1,1) @A
//> 0: sui::coin::mint<test::usdc::USDC>(Input(1), Input(0));
//> 1: sui::coin::into_balance<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(2));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(9500) @B
// Reject: withdraw 9500 from 10000, leaving 500 (below min 1000) - fails at voting time
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(3000) @B 2500 @A
// Reject: withdraw 3000, send 2500 to B, send 500 back to self (below min) - fails at execution
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::split<test::usdc::USDC>(Result(0), Input(2));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(1));
//> 3: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(3));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(2000) @B 1500u256
// Reject: withdrawal_split 1500 from 2000, drop remaining 500 (below min) - fails at execution
//> 0: sui::funds_accumulator::withdrawal_split<sui::balance::Balance<test::usdc::USDC>>(Input(0), Input(2));
//> 1: sui::balance::redeem_funds<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(1));
