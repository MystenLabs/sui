// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A SenderAllowance spend from a frozen funder is denied. Simulator mode skips the signing check,
// so execution decides.

//# init --accounts A B C --addresses test=0x0 --simulator

//# publish --sender A
module test::regulated_coin;

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
    let coin = treasury_cap.mint(10000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(cap, ctx.sender());
    transfer::public_transfer(metadata_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
}

//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

// Setup: the funder B's address balance.
//# programmable --sender A --inputs object(1,5) 1000 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2))

//# create-checkpoint

// Setup: B grants C an allowance on its balance.
//# programmable --sender B --inputs b"" @C vector[1000u256] vector[] vector[99999999999999]
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

// Control: C spends while B is not frozen.
//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(100,@B,object(5,0)) mutshared(5,0) immshared(6) @C
//> 0: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(3));

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Setup: a SeizeAuth for B held by the spender C.
//# programmable --sender A --inputs object(0xcf) object(1,3) @B @C
//> 0: sui::coin_regulation::issue_seize_auth<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2));
//> sui::coin_regulation::transfer_seize_auth<test::regulated_coin::REGULATED_COIN>(Result(0), Input(3))

// C spends from the frozen funder into its balance.
//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(100,@B,object(5,0)) mutshared(5,0) immshared(6) @C
//> 0: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(3));

// C spends from the frozen funder into a Coin.
//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(100,@B,object(5,0)) mutshared(5,0) immshared(6) @C
//> 0: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(1), Input(0), Input(2));
//> 1: sui::coin::from_balance<test::regulated_coin::REGULATED_COIN>(Result(0));
//> 2: TransferObjects([Result(1)], Input(3));

//# advance-epoch

// The allowance spend with a SeizeAuth for B, in a later epoch, as an unused input.
//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(100,@B,object(5,0)) mutshared(5,0) immshared(6) @C object(8,0)
//> 0: sui::allowance::balance_spend<test::regulated_coin::REGULATED_COIN>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(3));
