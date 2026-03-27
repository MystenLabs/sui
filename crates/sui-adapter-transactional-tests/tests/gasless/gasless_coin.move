// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests gasless transactions with coin object inputs, MergeCoins, and SplitCoins.

//# init --addresses test=0x0 --accounts A B C --enable-gasless --enable-accumulators --shared-object-deletion true

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

    public fun freeze_coin(coin: Coin<USDC>) {
        transfer::public_freeze_object(coin);
    }

    public fun share_coin(coin: Coin<USDC>) {
        transfer::public_share_object(coin);
    }
}

//# gasless-allow-token test::usdc::USDC

//# programmable --sender A --inputs 5000 object(1,1) @A
// Mint three USDC coins for A
//> 0: sui::coin::mint<test::usdc::USDC>(Input(1), Input(0));
//> 1: sui::coin::mint<test::usdc::USDC>(Input(1), Input(0));
//> 2: sui::coin::mint<test::usdc::USDC>(Input(1), Input(0));
//> TransferObjects([Result(0), Result(1), Result(2)], Input(2));

//# create-checkpoint

// --- Test 1: Simple coin input sent via coin::send_funds ---

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs object(3,0) @B
// Gasless: send a coin object to B via coin::send_funds
//> 0: sui::coin::send_funds<test::usdc::USDC>(Input(0), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> B

// --- Test 2: MergeCoins then coin::send_funds ---

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs object(3,1) object(3,2) @C
// Gasless: merge two coins then send the result to C
//> 0: MergeCoins(Input(0), [Input(1)]);
//> 1: sui::coin::send_funds<test::usdc::USDC>(Input(0), Input(2));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> C

// --- Test 3: Mint fresh coins, SplitCoins then send parts to different recipients ---

//# programmable --sender A --inputs 9000 object(1,1) @A
// Mint one large coin for A
//> 0: sui::coin::mint<test::usdc::USDC>(Input(1), Input(0));
//> TransferObjects([Result(0)], Input(2));

//# create-checkpoint

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs object(11,0) 3000 @B @C
// Gasless: split 3000 from 9000 coin, send split to B and remainder to C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: sui::coin::send_funds<test::usdc::USDC>(NestedResult(0, 0), Input(2));
//> 2: sui::coin::send_funds<test::usdc::USDC>(Input(0), Input(3));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> B

//# view-funds sui::balance::Balance<test::usdc::USDC> C

// --- Test 4: Immutable coin input rejected ---

//# programmable --sender A --inputs object(1,1) 1000
// Mint and freeze a coin
//> 0: sui::coin::mint<test::usdc::USDC>(Input(0), Input(1));
//> test::usdc::freeze_coin(Result(0));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs object(17,0) @B
// Reject: immutable coin
//> 0: sui::coin::send_funds<test::usdc::USDC>(Input(0), Input(1));

// --- Test 5: Shared coin input rejected ---

//# programmable --sender A --inputs object(1,1) 1000
// Mint and share a coin
//> 0: sui::coin::mint<test::usdc::USDC>(Input(0), Input(1));
//> test::usdc::share_coin(Result(0));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs object(19,0) @B
// Reject: shared coin (used)
//> 0: sui::coin::send_funds<test::usdc::USDC>(Input(0), Input(1));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs object(19,0) @B
// Reject: shared coin (unused)
//> 0: sui::balance::zero<test::usdc::USDC>();
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));

// --- Test 6: Party coin input succeeds (used and unused) ---

//# programmable --sender A --inputs object(1,1) 2000 @A
// Mint two coins and party-transfer them
//> 0: sui::coin::mint<test::usdc::USDC>(Input(0), Input(1));
//> 1: sui::coin::mint<test::usdc::USDC>(Input(0), Input(1));
//> 2: sui::party::single_owner(Input(2));
//> sui::transfer::public_party_transfer<sui::coin::Coin<test::usdc::USDC>>(Result(0), Result(2));
//> sui::transfer::public_party_transfer<sui::coin::Coin<test::usdc::USDC>>(Result(1), Result(2));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs object(22,0) @B
// Success: party coin input used
//> 0: sui::coin::send_funds<test::usdc::USDC>(Input(0), Input(1));

//# programmable --sender A --address-balance-gas --gas-price 0 --gas-budget 0 --inputs object(22,1) @B
// Reject: party coin input unused, fails post-execution check
//> 0: sui::balance::zero<test::usdc::USDC>();
//> 1: sui::balance::send_funds<test::usdc::USDC>(Result(0), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<test::usdc::USDC> B
