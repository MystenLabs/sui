// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A frozen address cannot send or receive the coin, as a Coin or as an address balance. SUI is
// unaffected. The coin is made with `make_compliance_regulated`, so the first freeze adds its FreezeList
// state.

//# init --accounts A B C --addresses test=0x0

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

//# view-object 1,5

//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2))

//# programmable --sender B --inputs object(4,0) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// A Coin in an object's dynamic field, which signing cannot see.
//# programmable --sender B --inputs object(4,0) 10 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> test::regulated_coin::wrap(Result(0), Input(2))

//# create-checkpoint

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Frozen sender: Coin.
//# programmable --sender B --inputs object(4,0) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Frozen sender: Coin held in an object, decided by execution.
//# run test::regulated_coin::split_wrapped --args object(7,0) 1 @A --sender B

// Frozen sender: address balance.
//# programmable --sender B --inputs withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(1) @A
//> 0: sui::balance::redeem_funds<test::regulated_coin::REGULATED_COIN>(Input(0));
//> 1: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(1));

// Frozen sender: address balance funded from a Coin held in an object, decided by execution.
//# run test::regulated_coin::send_wrapped_funds --args object(7,0) 1 @A --sender B

// Frozen recipient: Coin.
//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Frozen recipient: address balance.
//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2))

// Unfrozen recipients still work.
//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2))

// SUI to and from the frozen address is unaffected and does not read the FreezeList.
//# programmable --sender B --inputs 10 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender B --inputs 10 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1))

//# programmable --sender A --inputs 10 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 10 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1))
