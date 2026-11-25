// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A declared withdrawal from a frozen funder is denied even when no command redeems it. Simulator
// mode skips the signing check, so execution decides.

//# init --accounts A B --addresses test=0x0 --simulator

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

// Setup: B's address balance.
//# programmable --sender A --inputs object(1,5) 1000 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2))

//# create-checkpoint

// Control: B declares a withdrawal and never redeems it.
//# programmable --sender B --inputs withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(1) @B
//> sui::address::to_u256(Input(1))

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender B --inputs withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(1) @B
//> sui::address::to_u256(Input(1))
