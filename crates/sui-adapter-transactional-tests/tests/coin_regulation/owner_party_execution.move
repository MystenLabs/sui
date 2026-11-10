// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A party Coin, under a frozen coin ID or a frozen owner. Simulator mode skips the signing check,
// so execution decides.

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

// Setup: two party Coins owned by C. The first one's ID is frozen here.
//# programmable --sender A --inputs object(1,5) 100 @C object(0xcf) object(1,3)
//> 0: SplitCoins(Input(0), [Input(1), Input(1)]);
//> 1: sui::object::id_address<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>(NestedResult(0,0));
//> 2: sui::coin_regulation::freeze_address<test::regulated_coin::REGULATED_COIN>(Input(3), Input(4), Result(1));
//> 3: sui::party::single_owner(Input(2));
//> 4: sui::transfer::public_party_transfer<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>(NestedResult(0,0), Result(3));
//> 5: sui::party::single_owner(Input(2));
//> sui::transfer::public_party_transfer<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>(NestedResult(0,1), Result(5))

// Control: C moves T out of and into its unfrozen party Coin.
//# programmable --sender C --inputs object(3,1) 1 @C
//> 0: SplitCoins(Input(0), [Input(1), Input(1)]);
//> 1: MergeCoins(Input(0), [NestedResult(0,1)]);
//> TransferObjects([NestedResult(0,0)], Input(2))

// T moves out of a party Coin whose ID is frozen.
//# programmable --sender C --inputs object(3,0) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// T moves into a party Coin whose ID is frozen.
//# programmable --sender C --inputs object(3,0) object(3,1) 1
//> 0: SplitCoins(Input(1), [Input(2)]);
//> MergeCoins(Input(0), [Result(0)])

// Control: A sends a party Coin to C.
//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: sui::party::single_owner(Input(2));
//> sui::transfer::public_party_transfer<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>(Result(0), Result(1))

// Setup: freeze C.
//# programmable --sender A --inputs object(0xcf) object(1,3) @C
//> sui::coin_regulation::freeze_address<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2))

// A sends a party Coin to the frozen C.
//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: sui::party::single_owner(Input(2));
//> sui::transfer::public_party_transfer<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>(Result(0), Result(1))

// The frozen C moves T out of its unfrozen party Coin.
//# programmable --sender C --inputs object(3,1) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))
