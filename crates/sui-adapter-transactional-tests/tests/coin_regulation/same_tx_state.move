// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Freeze and pause checks never see the transaction's own FreezeList writes.

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

// A Coin in an object's dynamic field, which signing cannot see.
//# programmable --sender A --inputs object(1,5) 100 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> test::regulated_coin::wrap(Result(0), Input(2))

// Freeze-and-use passes: the check reads the FreezeList as it was before the transaction.
//# programmable --sender A --inputs object(0xcf) object(1,3) object(1,5) 1 @C
//> 0: sui::coin_regulation::freeze_address<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(4));
//> 1: SplitCoins(Input(2), [Input(3)]);
//> TransferObjects([Result(1)], Input(4))

//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Pause-and-use passes.
//# programmable --sender A --inputs object(0xcf) object(1,3) object(1,5) 1 @A
//> 0: sui::coin_regulation::enable_global_pause<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1));
//> 1: SplitCoins(Input(2), [Input(3)]);
//> TransferObjects([Result(1)], Input(4))

//# programmable --sender A --inputs object(1,5) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Paused, decided by execution.
//# run test::regulated_coin::split_wrapped --args object(3,0) 1 @A --sender A

// Unpause-and-use fails.
//# programmable --sender A --inputs object(0xcf) object(1,3) object(1,5) 1 @A
//> 0: sui::coin_regulation::disable_global_pause<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1));
//> 1: SplitCoins(Input(2), [Input(3)]);
//> TransferObjects([Result(1)], Input(4))

// Unpause-and-use fails in execution too.
//# programmable --sender A --inputs object(0xcf) object(1,3) object(3,0) 1 @A
//> 0: sui::coin_regulation::disable_global_pause<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1));
//> 1: test::regulated_coin::split_wrapped(Input(2), Input(3), Input(4))

//# run sui::coin_regulation::disable_global_pause --args object(0xcf) object(1,3) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(1,5) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(0xcf) object(1,3) @C @A
//> 0: sui::coin_regulation::issue_seize_auth<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2));
//> sui::coin_regulation::transfer_seize_auth<test::regulated_coin::REGULATED_COIN>(Result(0), Input(3))

// Destroying the only SeizeAuth and unfreezing in one transaction aborts.
//# programmable --sender A --inputs object(0xcf) object(1,3) object(13,0) object(1,5) 1 @C
//> 0: sui::coin_regulation::destroy_seize_auth<test::regulated_coin::REGULATED_COIN>(Input(0), Input(2));
//> 1: sui::coin_regulation::unfreeze_address<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(5));
//> 2: SplitCoins(Input(3), [Input(4)]);
//> TransferObjects([Result(2)], Input(5))

//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Unfreeze-and-use without a SeizeAuth fails.
//# programmable --sender A --inputs object(0xcf) object(1,3) object(1,5) 1 @B
//> 0: sui::coin_regulation::unfreeze_address<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(4));
//> 1: SplitCoins(Input(2), [Input(3)]);
//> TransferObjects([Result(1)], Input(4))
