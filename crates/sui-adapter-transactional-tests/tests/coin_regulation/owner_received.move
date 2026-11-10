// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A Coin received from an object, under a frozen parent ID or a frozen coin ID.

//# init --accounts A --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::coin::Coin;
use sui::coin_registry;
use sui::transfer::Receiving;

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

public fun new_parent(ctx: &mut TxContext) {
    transfer::transfer(Parent { id: object::new(ctx) }, ctx.sender());
}

public fun receive(
    parent: &mut Parent,
    coin: Receiving<Coin<REGULATED_COIN>>,
): Coin<REGULATED_COIN> {
    transfer::public_receive(&mut parent.id, coin)
}
//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

// Setup: two parents.
//# programmable --sender A
//> 0: test::regulated_coin::new_parent();
//> test::regulated_coin::new_parent()

// Setup: two Coins at the first parent's address, and one at the second's whose ID is frozen here.
//# programmable --sender A --inputs object(1,5) 100 object(3,0) object(3,1) object(0xcf) object(1,3)
//> 0: SplitCoins(Input(0), [Input(1), Input(1), Input(1)]);
//> 1: sui::object::id_address<test::regulated_coin::Parent>(Input(2));
//> 2: sui::object::id_address<test::regulated_coin::Parent>(Input(3));
//> 3: sui::object::id_address<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>(NestedResult(0,2));
//> 4: sui::coin_regulation::freeze_address<test::regulated_coin::REGULATED_COIN>(Input(4), Input(5), Result(3));
//> 5: TransferObjects([NestedResult(0,0), NestedResult(0,1)], Result(1));
//> TransferObjects([NestedResult(0,2)], Result(2))

// Control: A receives a Coin from the first parent.
//# programmable --sender A --inputs object(3,0) receiving(4,0) @A
//> 0: test::regulated_coin::receive(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

// A receives a Coin whose ID is frozen.
//# programmable --sender A --inputs object(3,1) receiving(4,2) @A
//> 0: test::regulated_coin::receive(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

// Setup: freeze the first parent's ID.
//# programmable --sender A --inputs object(0xcf) object(1,3) object(3,0)
//> 0: sui::object::id_address<test::regulated_coin::Parent>(Input(2));
//> sui::coin_regulation::freeze_address<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Result(0))

// A receives a Coin sent to a frozen parent.
//# programmable --sender A --inputs object(3,0) receiving(4,1) @A
//> 0: test::regulated_coin::receive(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

// A sends T to the frozen parent's address.
//# programmable --sender A --inputs object(1,5) 1 object(3,0)
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: sui::object::id_address<test::regulated_coin::Parent>(Input(2));
//> TransferObjects([Result(0)], Result(1))
