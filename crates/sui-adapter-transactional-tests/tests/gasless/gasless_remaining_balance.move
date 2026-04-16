// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests dust prevention: withdrawals must not leave a remaining balance
// between 0 and min_transfer in the sender's account.
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

//# programmable --sender A --inputs 5000 object(1,1) @A
//> 0: sui::coin::mint<test::usdc::USDC>(Input(1), Input(0));
//> 1: sui::coin::into_balance<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(2));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(4500) @B
// Withdraw 4500 from 5000, leaving 500 dust - should fail at voting time
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(4000) @B
// Withdraw 4000 from 5000, leaving 1000 (== min) - should succeed
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(1000) @B
// Withdraw all remaining 1000, leaving 0 - should succeed
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# programmable --sender A --inputs 10000 object(1,1) @A
// Refund A for send-back-to-self tests
//> 0: sui::coin::mint<test::usdc::USDC>(Input(1), Input(0));
//> 1: sui::coin::into_balance<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(2));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(3000) @B 2500 @A
// Withdraw 3000, send 2500 to B, send 500 (dust) back to self - should fail at execution
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::split<test::usdc::USDC>(Result(0), Input(2));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(1));
//> 3: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(3));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(3000) @B 2000 @A
// Withdraw 3000, send 2000 to B, send 1000 back to self (>= min) - should succeed
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::split<test::usdc::USDC>(Result(0), Input(2));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(1));
//> 3: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(3));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> A

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(2000) @B 1500u256
// withdrawal_split 1500 from 2000 withdrawal, redeem only 1500, send to B. 500 dropped as dust.
//> 0: sui::funds_accumulator::withdrawal_split<sui::balance::Balance<test::usdc::USDC>>(Input(0), Input(2));
//> 1: sui::balance::redeem_funds<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(1));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(2000) @B 1000u256
// withdrawal_split 1000 from 2000, redeem both halves, send both to B. Nothing dropped.
//> 0: sui::funds_accumulator::withdrawal_split<sui::balance::Balance<test::usdc::USDC>>(Input(0), Input(2));
//> 1: sui::balance::redeem_funds<test::usdc::USDC>(Result(0));
//> 2: sui::balance::send_funds<test::usdc::USDC>(Result(1), Input(1));
//> 3: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 4: sui::balance::send_funds<test::usdc::USDC>(Result(3), Input(1));
