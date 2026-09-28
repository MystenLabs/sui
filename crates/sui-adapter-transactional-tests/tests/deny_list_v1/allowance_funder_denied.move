// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A spend through an allowance debits the funder rather than the sender, so a
// denied funder is rejected at signing even though the sender is clear.

//# init --accounts A B C --addresses test=0x0

//# publish --sender A
#[allow(deprecated_usage)]
module test::regulated_coin;

use sui::coin;

public struct REGULATED_COIN has drop {}

fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency(
        otw,
        9,
        b"RC",
        b"REGULATED_COIN",
        b"A new regulated coin",
        option::none(),
        ctx,
    );
    let coin = coin::mint(&mut treasury_cap, 10000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
    transfer::public_freeze_object(metadata);
}

//# programmable --sender A --inputs object(1,5) 1000 @B
// Fund B's (the funder) address balance.
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2));

//# create-checkpoint

//# programmable --sender B --inputs b"" @C vector[1000u256] vector[] vector[99999999999999]
// B issues an allowance to C.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(100,@B,object(4,0)) mutshared(4,0) immshared(6) @C
// C spends while nobody is denied.
//> 0: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(1), Input(0), Input(2));
//> 1: sui::coin::from_balance<test::regulated_coin::REGULATED_COIN>(Result(0));
//> 2: TransferObjects([Result(1)], Input(3));

//# run sui::coin::deny_list_add --args object(0x403) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(100,@B,object(4,0)) mutshared(4,0) immshared(6) @C
// The same spend is rejected at signing, naming the funder.
//> 0: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(1), Input(0), Input(2));
//> 1: sui::coin::from_balance<test::regulated_coin::REGULATED_COIN>(Result(0));
//> 2: TransferObjects([Result(1)], Input(3));
