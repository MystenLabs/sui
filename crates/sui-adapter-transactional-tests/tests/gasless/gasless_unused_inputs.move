// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests gasless transaction validation rejects unused inputs and oversized Pure inputs.
// This tests:
// 1. Too many unused Pure inputs (limit set to 1)
// 2. Unused FundsWithdrawal inputs (always rejected)
// 3. Unused Object inputs (always rejected)
// 4. Pure inputs that exceed the size limit (32 bytes)

//# init --addresses test=0x0 --accounts A B --enable-gasless --enable-accumulators --gasless-max-pure-input-bytes 32 --gasless-max-unused-inputs 1

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

// ============================================================================
// Test 1: Too many unused Pure inputs - should FAIL
// We allow 1 unused Pure input, but this has 2 (Input(2) and Input(3))
// ============================================================================
//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(500) @B 42u64 99u64
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));

// ============================================================================
// Test 2: Unused FundsWithdrawal input - should FAIL
// Input(1) is a FundsWithdrawal that is not used by any command
// ============================================================================
//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(500) withdraw<sui::balance::Balance<test::usdc::USDC>>(500) @B
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(2));

// ============================================================================
// Test 3: Unused Object input - should FAIL
// Input(0) is an Object (TreasuryCap) that is not used by any command
// ============================================================================
//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs object(1,1) withdraw<sui::balance::Balance<test::usdc::USDC>>(500) @B
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(1));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(2));

// ============================================================================
// Test 4: Pure input too large (> 32 bytes) - should FAIL
// Input(1) is a 33-byte vector which exceeds the 32 byte limit
// ============================================================================
//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs withdraw<sui::balance::Balance<test::usdc::USDC>>(500) vector[1u8,2u8,3u8,4u8,5u8,6u8,7u8,8u8,9u8,10u8,11u8,12u8,13u8,14u8,15u8,16u8,17u8,18u8,19u8,20u8,21u8,22u8,23u8,24u8,25u8,26u8,27u8,28u8,29u8,30u8,31u8,32u8,33u8]
//> 0: sui::balance::redeem_funds<test::usdc::USDC>(Input(0));
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));
