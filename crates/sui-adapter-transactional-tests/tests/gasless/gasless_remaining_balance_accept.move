// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that gasless withdrawals not leaving a balance below the minimum are accepted.
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

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(9000) @B
// Accept: withdraw 9000 from 10000, leaving 1000 (== min)
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(1000) @B
// Accept: withdraw all remaining 1000, leaving 0
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# programmable --sender A --inputs 10000 object(1,1) @A
//> 0: sui::coin::mint<test::usdc::USDC>(Input(1), Input(0));
//> 1: sui::coin::into_balance<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(2));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(3000) @B 2000 @A
// Accept: withdraw 3000, send 2000 to B, send 1000 back to self (>= min)
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::split<test::usdc::USDC>(Result(0), Input(2));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(1));
//> 3: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(3));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(2000) @B 1000u256
// Accept: withdrawal_split 1000 from 2000, redeem both halves, send both to B
//> 0: sui::funds_accumulator::withdrawal_split<sui::balance::Balance<test::usdc::USDC>>(Input(0), Input(2));
//> 1: sui::balance::redeem_funds<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(1));
//> 3: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 4: sui::balance::send_funds<test::usdc::USDC>(Result(3), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A
