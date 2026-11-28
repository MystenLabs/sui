// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A frozen object involved in a transaction blocks it whenever the transaction touches the coin,
// even if the frozen object itself holds none.

//# init --accounts A --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::balance::Balance;
use sui::coin::{Self, Coin};
use sui::coin_registry;
use sui::coin_regulation::{ComplianceCap, FreezeList};
use sui::derived_object;
use sui::dynamic_field as df;
use sui::dynamic_object_field as dof;

public struct REGULATED_COIN has drop {}

public struct Empty has key, store {
    id: UID,
}

public struct Wrapper has key {
    id: UID,
    inner: Empty,
}

public struct Parent has key {
    id: UID,
}

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

public fun new_empty(ctx: &mut TxContext) {
    transfer::transfer(Empty { id: object::new(ctx) }, ctx.sender());
}

public fun new_wrapper(ctx: &mut TxContext) {
    let inner = Empty { id: object::new(ctx) };
    transfer::transfer(Wrapper { id: object::new(ctx), inner }, ctx.sender());
}

public fun new_parent(ctx: &mut TxContext): Parent {
    Parent { id: object::new(ctx) }
}

public fun add_child(parent: &mut Parent, ctx: &mut TxContext) {
    dof::add(&mut parent.id, 0u64, Empty { id: object::new(ctx) });
}

public fun add_balance(parent: &mut Parent, coin: Coin<REGULATED_COIN>) {
    df::add(&mut parent.id, 0u64, coin.into_balance());
}

public fun keep(parent: Parent, ctx: &TxContext) {
    transfer::transfer(parent, ctx.sender());
}

public fun freeze_object<O: key>(
    list: &mut FreezeList,
    cap: &mut ComplianceCap<REGULATED_COIN>,
    obj: &O,
    ctx: &mut TxContext,
) {
    list.freeze_address(cap, object::id_address(obj), ctx);
}

public fun freeze_wrapped(
    list: &mut FreezeList,
    cap: &mut ComplianceCap<REGULATED_COIN>,
    wrapper: &Wrapper,
    ctx: &mut TxContext,
) {
    list.freeze_address(cap, object::id_address(&wrapper.inner), ctx);
}

public fun freeze_child(
    list: &mut FreezeList,
    cap: &mut ComplianceCap<REGULATED_COIN>,
    parent: &Parent,
    ctx: &mut TxContext,
) {
    list.freeze_address(cap, object::id_address(dof::borrow<u64, Empty>(&parent.id, 0)), ctx);
}

public fun freeze_derived(
    list: &mut FreezeList,
    cap: &mut ComplianceCap<REGULATED_COIN>,
    parent: &Parent,
    ctx: &mut TxContext,
) {
    list.freeze_address(cap, derived_object::derive_address(object::id(parent), 0u64), ctx);
}

public fun read<O: key>(_: &O) {}

public fun read_child(parent: &Parent) {
    let _: &Empty = dof::borrow(&parent.id, 0u64);
}

public fun take_from_field(
    parent: &mut Parent,
    amount: u64,
    ctx: &mut TxContext,
): Coin<REGULATED_COIN> {
    let balance: &mut Balance<REGULATED_COIN> = df::borrow_mut(&mut parent.id, 0u64);
    coin::take(balance, amount, ctx)
}

public fun claim_and_release(parent: &mut Parent) {
    let id = derived_object::claim(&mut parent.id, 0u64);
    derived_object::release(&mut parent.id, id);
}

//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

// Unrelated object holding no coin.
//# run test::regulated_coin::new_empty --sender A

//# run test::regulated_coin::new_wrapper --sender A

// Parent of a dynamic object field child.
//# programmable --sender A
//> 0: test::regulated_coin::new_parent();
//> 1: test::regulated_coin::add_child(Result(0));
//> 2: test::regulated_coin::keep(Result(0));

// Parent of a dynamic field holding the coin's balance.
//# programmable --sender A --inputs object(1,5) 100
//> 0: test::regulated_coin::new_parent();
//> 1: SplitCoins(Input(0), [Input(1)]);
//> 2: test::regulated_coin::add_balance(Result(0), Result(1));
//> 3: test::regulated_coin::keep(Result(0));

// Parent for a derived object.
//# programmable --sender A
//> 0: test::regulated_coin::new_parent();
//> 1: test::regulated_coin::keep(Result(0));

// Secondary gas coins, one per use since gas smashing deletes them.
//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0), Input(0)]);
//> TransferObjects([NestedResult(0, 0), NestedResult(0, 1)], Input(1))

// An unfrozen parent of a dynamic field holding the coin's balance, which signing cannot see.
//# programmable --sender A --inputs object(1,5) 100
//> 0: test::regulated_coin::new_parent();
//> 1: SplitCoins(Input(0), [Input(1)]);
//> 2: test::regulated_coin::add_balance(Result(0), Result(1));
//> 3: test::regulated_coin::keep(Result(0));

//# view-object 3,0

//# view-object 4,0

//# view-object 5,0

//# view-object 6,0

//# view-object 7,0

//# view-object 8,0

//# view-object 8,1

// Every case passes before any freeze.
//# programmable --sender A --inputs object(3,0) object(4,0) object(5,0) object(6,0) object(7,0) object(1,5) 1 @A
//> 0: test::regulated_coin::read<test::regulated_coin::Empty>(Input(0));
//> 1: test::regulated_coin::read<test::regulated_coin::Wrapper>(Input(1));
//> 2: test::regulated_coin::read_child(Input(2));
//> 3: test::regulated_coin::take_from_field(Input(3), Input(6));
//> 4: test::regulated_coin::claim_and_release(Input(4));
//> 5: SplitCoins(Input(5), [Input(6)]);
//> TransferObjects([Result(3), Result(5)], Input(7))

//# programmable --sender A --inputs object(0xcf) object(1,3) object(3,0) object(4,0) object(5,0) object(6,0) object(7,0) object(8,0) object(8,1)
//> 0: test::regulated_coin::freeze_object<test::regulated_coin::Empty>(Input(0), Input(1), Input(2));
//> 1: test::regulated_coin::freeze_wrapped(Input(0), Input(1), Input(3));
//> 2: test::regulated_coin::freeze_child(Input(0), Input(1), Input(4));
//> 3: test::regulated_coin::freeze_object<test::regulated_coin::Parent>(Input(0), Input(1), Input(5));
//> 4: test::regulated_coin::freeze_derived(Input(0), Input(1), Input(6));
//> 5: test::regulated_coin::freeze_object<sui::coin::Coin<sui::sui::SUI>>(Input(0), Input(1), Input(7));
//> 6: test::regulated_coin::freeze_object<sui::coin::Coin<sui::sui::SUI>>(Input(0), Input(1), Input(8));

// Touching the coin alone still works.
//# programmable --sender A --inputs object(1,5) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Unrelated frozen input object holding no coin.
//# programmable --sender A --inputs object(3,0) object(1,5) 1 @A
//> 0: test::regulated_coin::read<test::regulated_coin::Empty>(Input(0));
//> 1: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(1)], Input(3))

// The same, touching the coin only through a dynamic field, decided by execution.
//# programmable --sender A --inputs object(3,0) object(9,0) 1 @A
//> 0: test::regulated_coin::read<test::regulated_coin::Empty>(Input(0));
//> 1: test::regulated_coin::take_from_field(Input(1), Input(2));
//> TransferObjects([Result(1)], Input(3))

// The same frozen object without the coin is fine.
//# programmable --sender A --inputs object(3,0)
//> 0: test::regulated_coin::read<test::regulated_coin::Empty>(Input(0));

// Frozen object wrapped in an input. Its ID has no fake name, so the error prints it as _.
//# programmable --sender A --inputs object(4,0) object(1,5) 1 @A
//> 0: test::regulated_coin::read<test::regulated_coin::Wrapper>(Input(0));
//> 1: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(1)], Input(3))

// Frozen object loaded as a dynamic object field child.
//# programmable --sender A --inputs object(5,0) object(1,5) 1 @A
//> 0: test::regulated_coin::read_child(Input(0));
//> 1: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(1)], Input(3))

// Frozen parent of the dynamic field holding the balance.
//# programmable --sender A --inputs object(6,0) 1 @A
//> 0: test::regulated_coin::take_from_field(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

// Frozen derived object created and deleted in the transaction, printed as _.
//# programmable --sender A --inputs object(7,0) object(1,5) 1 @A
//> 0: test::regulated_coin::claim_and_release(Input(0));
//> 1: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(1)], Input(3))

//# programmable --sender A --inputs object(7,0)
//> 0: test::regulated_coin::claim_and_release(Input(0));

// Signing rejects a frozen secondary gas coin, though execution smashes it before the PTB.
//# programmable --sender A --inputs object(1,5) 1 @A --gas-payment object(0,0) --gas-payment object(8,0)
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// The same, touching the coin only through a dynamic field, decided by execution.
//# programmable --sender A --inputs object(9,0) 1 @A --gas-payment object(0,0) --gas-payment object(8,1)
//> 0: test::regulated_coin::take_from_field(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))
