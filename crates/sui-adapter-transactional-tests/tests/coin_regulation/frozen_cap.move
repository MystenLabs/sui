// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A frozen ComplianceCap cannot be used, by its issuer or by anyone else.

//# init --accounts A B C --addresses test=0x0

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

// Control: while A owns the cap, B cannot use it.
//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @C --type-args test::regulated_coin::REGULATED_COIN --sender B

//# run sui::transfer::public_freeze_object --args object(1,3) --type-args sui::coin_regulation::ComplianceCap<test::regulated_coin::REGULATED_COIN> --sender A

//# view-object 1,3

// Neither B nor A can use the frozen cap.
//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @C --type-args test::regulated_coin::REGULATED_COIN --sender B

//# run sui::coin_regulation::enable_global_pause --args object(0xcf) object(1,3) --type-args test::regulated_coin::REGULATED_COIN --sender B

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @C --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin_regulation::enable_global_pause --args object(0xcf) object(1,3) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(0xcf) object(1,3) @C
//> 0: sui::coin_regulation::issue_seize_auth<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2));
//> sui::coin_regulation::transfer_seize_auth<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2))

// The coin stays unrestricted.
//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))
