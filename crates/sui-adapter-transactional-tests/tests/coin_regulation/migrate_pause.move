// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The legacy global pause carries over, and a FreezeList unpause ends it while the legacy list
// is still paused.

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::coin;
use sui::coin_regulation::{Self, FreezeList};
use sui::dynamic_field;

public struct REGULATED_COIN has drop {}

#[allow(deprecated_usage)]
fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
        otw,
        9,
        b"RC",
        b"REGULATED_COIN",
        b"A new regulated coin",
        option::none(),
        true,
        ctx,
    );
    let coin = coin::mint(&mut treasury_cap, 10000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
    transfer::public_freeze_object(metadata);
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

public fun assert_paused(list: &FreezeList, expected: bool) {
    assert!(coin_regulation::is_global_pause_enabled<REGULATED_COIN>(list) == expected);
}

//# run sui::coin_registry::migrate_legacy_metadata --args object(0xc) object(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

// A Coin in an object's dynamic field, which signing cannot see.
//# programmable --sender A --inputs object(1,5) 100 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> test::regulated_coin::wrap(Result(0), Input(2))

//# run sui::coin::deny_list_v2_enable_global_pause --args object(0x403) object(1,3) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# advance-epoch

//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(2,0) object(0xcf) object(0x403) object(1,3) vector[@B] @A
//> 0: sui::coin_registry::migrate_to_compliance_cap<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2), Input(3), Input(4));
//> TransferObjects([Result(0)], Input(5))

//# run test::regulated_coin::assert_paused --args immshared(0xcf) true --sender A

//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// The carried-over pause, decided by execution.
//# run test::regulated_coin::split_wrapped --args object(3,0) 1 @B --sender A

//# run sui::coin_regulation::disable_global_pause --args object(0xcf) object(1,3) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run test::regulated_coin::assert_paused --args immshared(0xcf) false --sender A

//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))
