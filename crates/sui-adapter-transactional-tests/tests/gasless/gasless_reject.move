// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests gasless transaction validation rejects invalid transactions.

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

#[allow(deprecated_usage)]
module test::not_allowed {
    use sui::coin;

    public struct NOT_ALLOWED has drop {}

    fun init(otw: NOT_ALLOWED, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency(
            otw, 6, b"NA", b"Not Allowed", b"", option::none(), ctx,
        );
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, ctx.sender());
    }
}

// Only USDC is registered — NOT_ALLOWED is deliberately excluded
//# gasless-allow-token test::usdc::USDC

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs @B
// Reject: TransferObjects is not a MoveCall
//> TransferObjects([Gas], Input(0));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(1000)
// Reject: coin::from_balance is not a whitelisted gasless function
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::coin::from_balance<test::usdc::USDC>(Result(0));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(500)
// Reject: balance::value is not a whitelisted gasless function
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::value<test::usdc::USDC>(Result(0));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(500)
// Reject: funds_accumulator::withdrawal_owner is not a whitelisted gasless function
//> 0: sui::funds_accumulator::withdrawal_owner<sui::balance::Balance<test::usdc::USDC>>(Input(0));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 1000 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(500) @B
// Reject: nonzero gas budget
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(500) @B
// Reject: send_funds with zero type args
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds(Result(0), Input(1));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(500) @B
// Reject: send_funds with two type args
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC, test::usdc::USDC>(Result(0), Input(1));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(500) 250u256
// Reject: withdrawal_split with non-Balance type arg
//> 0: sui::funds_accumulator::withdrawal_split<test::usdc::USDC>(Input(0), Input(1));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::not_allowed::NOT_ALLOWED>>(500) @B
// Reject: NOT_ALLOWED token type is not in the gasless allowlist
//> 0: sui::balance::redeem_funds<test::not_allowed::NOT_ALLOWED>(Input(0));
//> 1: sui::balance::send_funds<test::not_allowed::NOT_ALLOWED>(Result(0), Input(1));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(500) @B
// Reject: non-framework package
//> 0: test::usdc::USDC(Input(0));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(500) @B
// Reject: mix of allowed and disallowed commands
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));
//> TransferObjects([Gas], Input(1));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs receiving(1,2) @B
// Reject: receiving object input
//> 0: sui::coin::send_funds<test::usdc::USDC>(Input(0), Input(1));
