// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A Coin owned by an object, under a frozen parent ID or a frozen coin ID.

//# init --accounts A --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::coin::Coin;
use sui::coin_registry;
use sui::dynamic_object_field as dof;

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

public struct Parent has key {
    id: UID,
}

public fun new_parent(coin: Coin<REGULATED_COIN>, ctx: &mut TxContext) {
    let mut parent = Parent { id: object::new(ctx) };
    dof::add(&mut parent.id, 0u64, coin);
    transfer::transfer(parent, ctx.sender());
}

public fun child_address(parent: &Parent): address {
    object::id_address(dof::borrow<u64, Coin<REGULATED_COIN>>(&parent.id, 0))
}

public fun take(parent: &mut Parent, amount: u64, ctx: &mut TxContext): Coin<REGULATED_COIN> {
    dof::borrow_mut<u64, Coin<REGULATED_COIN>>(&mut parent.id, 0).split(amount, ctx)
}

public fun put(parent: &mut Parent, coin: Coin<REGULATED_COIN>) {
    dof::borrow_mut<u64, Coin<REGULATED_COIN>>(&mut parent.id, 0).join(coin);
}
//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

// Setup: a parent whose own ID gets frozen, holding a Coin as a dynamic object field.
//# programmable --sender A --inputs object(1,5) 100
//> 0: SplitCoins(Input(0), [Input(1)]);
//> test::regulated_coin::new_parent(Result(0))

// Setup: a parent whose child Coin's ID gets frozen.
//# programmable --sender A --inputs object(1,5) 100
//> 0: SplitCoins(Input(0), [Input(1)]);
//> test::regulated_coin::new_parent(Result(0))

// Setup: the child Coin is owned by the dynamic object field object.
//# view-object 3,0

// Control: A moves T out of and into both child Coins.
//# programmable --sender A --inputs object(3,1) object(4,1) 1 2
//> 0: test::regulated_coin::take(Input(0), Input(2));
//> 1: test::regulated_coin::take(Input(1), Input(3));
//> 2: test::regulated_coin::put(Input(0), Result(1));
//> test::regulated_coin::put(Input(1), Result(0))

// Setup: freeze the first parent's ID and the second parent's child Coin's ID.
//# programmable --sender A --inputs object(0xcf) object(1,3) object(3,1) object(4,1)
//> 0: sui::object::id_address<test::regulated_coin::Parent>(Input(2));
//> 1: sui::coin_regulation::freeze_address<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Result(0));
//> 2: test::regulated_coin::child_address(Input(3));
//> 3: sui::coin_regulation::freeze_address<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Result(2))

// T moves out of a Coin owned by a frozen parent.
//# programmable --sender A --inputs object(3,1) 1 @A
//> 0: test::regulated_coin::take(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

// T moves into a Coin owned by a frozen parent.
//# programmable --sender A --inputs object(3,1) object(1,5) 1
//> 0: SplitCoins(Input(1), [Input(2)]);
//> test::regulated_coin::put(Input(0), Result(0))

// T moves out of an object-owned Coin whose ID is frozen.
//# programmable --sender A --inputs object(4,1) 1 @A
//> 0: test::regulated_coin::take(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

// T moves into an object-owned Coin whose ID is frozen.
//# programmable --sender A --inputs object(4,1) object(1,5) 1
//> 0: SplitCoins(Input(1), [Input(2)]);
//> test::regulated_coin::put(Input(0), Result(0))

// The frozen child Coin is only read.
//# programmable --sender A --inputs object(4,1)
//> 0: test::regulated_coin::child_address(Input(0))
