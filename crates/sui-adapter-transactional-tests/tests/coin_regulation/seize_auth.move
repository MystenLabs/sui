// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A SeizeAuth input does not exempt the frozen address it names, as receiver or as sender.

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::coin;
use sui::coin_registry;
use sui::dynamic_field;

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

public struct Wrapper has key {
    id: UID,
}

public fun wrap(coin: coin::Coin<REGULATED_COIN>, recipient: address, ctx: &mut TxContext) {
    let mut id = object::new(ctx);
    dynamic_field::add(&mut id, 0u8, coin);
    transfer::transfer(Wrapper { id }, recipient);
}

fun coin_mut(wrapper: &mut Wrapper): &mut coin::Coin<REGULATED_COIN> {
    dynamic_field::borrow_mut(&mut wrapper.id, 0u8)
}

public fun split_wrapped(
    wrapper: &mut Wrapper,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    transfer::public_transfer(wrapper.coin_mut().split(amount, ctx), recipient);
}

public fun send_wrapped_funds(
    wrapper: &mut Wrapper,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    wrapper.coin_mut().split(amount, ctx).send_funds(recipient);
}

//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// A Coin in an object's dynamic field, which signing cannot see.
//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> test::regulated_coin::wrap(Result(0), Input(2))

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(0xcf) object(1,3) @B @A
//> 0: sui::coin_regulation::issue_seize_auth<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2));
//> sui::coin_regulation::transfer_seize_auth<test::regulated_coin::REGULATED_COIN>(Result(0), Input(3))

//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(6,0) object(1,5) 1 @B
//> 0: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(0)], Input(3))

//# programmable --sender A --inputs object(0xcf) object(1,3) @B
//> 0: sui::coin_regulation::issue_seize_auth<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2));
//> sui::coin_regulation::transfer_seize_auth<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2))

//# programmable --sender B --inputs object(3,0) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Frozen sender, decided by execution.
//# run test::regulated_coin::split_wrapped --args object(4,0) 1 @A --sender B

//# programmable --sender B --inputs object(9,0) object(3,0) 1 @A
//> 0: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(0)], Input(3))

// Frozen sender holding its own SeizeAuth, decided by execution.
//# programmable --sender B --inputs object(9,0) object(4,0) 1 @A
//> 0: test::regulated_coin::split_wrapped(Input(1), Input(2), Input(3))
