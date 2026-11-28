// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Shared and immutable Coins, under a frozen coin ID or a frozen sender. Simulator mode skips the
// signing check, so execution decides.

//# init --accounts A B C --addresses test=0x0 --simulator

//# publish --sender A
module test::regulated_coin;

use sui::coin::Coin;
use sui::coin_registry;

public struct REGULATED_COIN has drop {}

fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    let (mut currency, mut treasury_cap) = coin_registry::new_currency_with_otw(
        otw,
        9,
        b"RC".to_string(),
        b"REGULATED_COIN".to_string(),
        b"A new regulated coin".to_string(),
        b"".to_string(),
        ctx,
    );
    let cap = currency.make_compliance_regulated(true, ctx);
    let metadata_cap = currency.finalize(ctx);
    let coin: Coin<REGULATED_COIN> = treasury_cap.mint(10000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(cap, ctx.sender());
    transfer::public_transfer(metadata_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
}

//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

// Setup: two shared Coins.
//# programmable --sender A --inputs object(1,5) 100
//> 0: SplitCoins(Input(0), [Input(1), Input(1)]);
//> 1: sui::transfer::public_share_object<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>(NestedResult(0,0));
//> 2: sui::transfer::public_share_object<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>(NestedResult(0,1))

// Setup: two immutable Coins.
//# programmable --sender A --inputs object(1,5) 100
//> 0: SplitCoins(Input(0), [Input(1), Input(1)]);
//> 1: sui::transfer::public_freeze_object<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>(NestedResult(0,0));
//> 2: sui::transfer::public_freeze_object<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>(NestedResult(0,1))

// Control: C takes T out of a shared Coin, adds T into it, and reads an immutable Coin.
//# programmable --sender C --inputs object(3,0) object(4,0) 1 @C
//> 0: SplitCoins(Input(0), [Input(2), Input(2)]);
//> 1: MergeCoins(Input(0), [NestedResult(0,1)]);
//> 2: sui::coin::value<test::regulated_coin::REGULATED_COIN>(Input(1));
//> TransferObjects([NestedResult(0,0)], Input(3))

// Setup: freeze the IDs of the first shared Coin and the first immutable Coin.
//# programmable --sender A --inputs object(0xcf) object(1,3) object(3,0) object(4,0)
//> 0: sui::object::id_address<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>(Input(2));
//> 1: sui::coin_regulation::freeze_address<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Result(0));
//> 2: sui::object::id_address<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>(Input(3));
//> 3: sui::coin_regulation::freeze_address<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Result(2))

// T moves out of a shared Coin whose ID is frozen.
//# programmable --sender C --inputs object(3,0) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// T moves into a shared Coin whose ID is frozen.
//# programmable --sender A --inputs object(3,0) object(1,5) 1
//> 0: SplitCoins(Input(1), [Input(2)]);
//> MergeCoins(Input(0), [Result(0)])

// C reads an immutable Coin whose ID is frozen.
//# programmable --sender C --inputs object(4,0)
//> 0: sui::coin::value<test::regulated_coin::REGULATED_COIN>(Input(0))

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// The frozen sender takes T out of an unfrozen shared Coin.
//# programmable --sender B --inputs object(3,1) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// The frozen sender reads an unfrozen immutable Coin.
//# programmable --sender B --inputs object(4,1)
//> 0: sui::coin::value<test::regulated_coin::REGULATED_COIN>(Input(0))

// C takes T out of an unfrozen shared Coin for the frozen B.
//# programmable --sender C --inputs object(3,1) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Control: the unfrozen shared and immutable Coins still work for C.
//# programmable --sender C --inputs object(3,1) object(4,1) 1 @C
//> 0: SplitCoins(Input(0), [Input(2)]);
//> 1: sui::coin::value<test::regulated_coin::REGULATED_COIN>(Input(1));
//> TransferObjects([Result(0)], Input(3))
