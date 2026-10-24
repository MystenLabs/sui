// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A re-claimed UID inherits its parent's root version, so the funds left in its dynamic fields are
// read at their latest versions. Older versions of the fields hold more SUI; reading one of them,
// or reading nothing, would let the funds be spent twice or overwritten.

//# init --addresses ex=0x0 --accounts A

//# publish
module ex::m;

use sui::balance::Balance;
use sui::coin::Coin;
use sui::derived_object;
use sui::dynamic_field as df;
use sui::dynamic_object_field as dof;
use sui::sui::SUI;

public struct Parent has key, store { id: UID }

public struct Root has key { id: UID }

public struct Holder has key { id: UID, parent: UID }

public struct Obj has key, store { id: UID }

public fun create_parent(ctx: &mut TxContext) {
    transfer::transfer(Parent { id: object::new(ctx) }, ctx.sender())
}

public fun create_root(ctx: &mut TxContext) {
    let mut root = Root { id: object::new(ctx) };
    dof::add(&mut root.id, 0u64, Parent { id: object::new(ctx) });
    transfer::transfer(root, ctx.sender())
}

public fun create_holder(ctx: &mut TxContext) {
    transfer::transfer(Holder { id: object::new(ctx), parent: object::new(ctx) }, ctx.sender())
}

public fun claim(parent: &mut Parent): Obj {
    Obj { id: derived_object::claim(&mut parent.id, 0u64) }
}

public fun release(parent: &mut Parent, obj: Obj) {
    let Obj { id } = obj;
    derived_object::release(&mut parent.id, id)
}

public fun claim_from_root(root: &mut Root): Obj {
    claim(dof::borrow_mut(&mut root.id, 0u64))
}

public fun release_from_root(root: &mut Root, obj: Obj) {
    release(dof::borrow_mut(&mut root.id, 0u64), obj)
}

public fun claim_from_holder(holder: &mut Holder): Obj {
    Obj { id: derived_object::claim(&mut holder.parent, 0u64) }
}

public fun release_from_holder(holder: &mut Holder, obj: Obj) {
    let Obj { id } = obj;
    derived_object::release(&mut holder.parent, id)
}

// Splits `coin` between a `Balance` dynamic field and a `Coin` dynamic object field.
public fun fund(obj: &mut Obj, mut coin: Coin<SUI>, ctx: &mut TxContext) {
    let amount = coin.value() / 2;
    let half = coin.split(amount, ctx);
    df::add(&mut obj.id, 0u64, coin.into_balance());
    dof::add(&mut obj.id, 1u64, half);
}

public fun deposit(obj: &mut Obj, coin: Coin<SUI>) {
    let balance: &mut Balance<SUI> = df::borrow_mut(&mut obj.id, 0u64);
    balance.join(coin.into_balance());
}

public fun withdraw(obj: &mut Obj, amount: u64, ctx: &mut TxContext): Coin<SUI> {
    let balance: &mut Balance<SUI> = df::borrow_mut(&mut obj.id, 0u64);
    balance.split(amount).into_coin(ctx)
}

public fun withdraw_coin(obj: &mut Obj, amount: u64, ctx: &mut TxContext): Coin<SUI> {
    let coin: &mut Coin<SUI> = dof::borrow_mut(&mut obj.id, 1u64);
    coin.split(amount, ctx)
}

public fun assert_funds(obj: &Obj, balance: u64, coin: u64) {
    assert!(df::borrow<u64, Balance<SUI>>(&obj.id, 0u64).value() == balance);
    assert!(dof::borrow<u64, Coin<SUI>>(&obj.id, 1u64).value() == coin);
}

public fun add_balance_field(obj: &mut Obj) {
    df::add(&mut obj.id, 0u64, sui::balance::zero<SUI>())
}

//# run ex::m::create_parent --sender A

// The balance field goes 500 -> 800 -> 200 and the coin field goes 500 -> 100.
//# programmable --sender A --inputs object(2,0) 1000 @A
//> 0: ex::m::claim(Input(0));
//> 1: SplitCoins(Gas, [Input(1)]);
//> ex::m::fund(Result(0), Result(1));
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(3,1) 300
//> 0: SplitCoins(Gas, [Input(1)]);
//> ex::m::deposit(Input(0), Result(0))

//# programmable --sender A --inputs object(3,1) 600 400 @A
//> 0: ex::m::withdraw(Input(0), Input(1));
//> 1: ex::m::withdraw_coin(Input(0), Input(2));
//> TransferObjects([Result(0), Result(1)], Input(3))

//# programmable --sender A --inputs object(2,0) object(3,1)
//> ex::m::release(Input(0), Input(1))

// Each of these re-claims X without loading it, so its field reads use the inherited bound.
// Withdrawing more than the latest balance aborts.
//# programmable --sender A --inputs object(2,0) 201 @A
//> 0: ex::m::claim(Input(0));
//> 1: ex::m::withdraw(Result(0), Input(1));
//> TransferObjects([Result(0), Result(1)], Input(2))

// Withdrawing more than the latest coin value aborts.
//# programmable --sender A --inputs object(2,0) 101 @A
//> 0: ex::m::claim(Input(0));
//> 1: ex::m::withdraw_coin(Result(0), Input(1));
//> TransferObjects([Result(0), Result(1)], Input(2))

// The balance field still exists.
//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim(Input(0));
//> ex::m::add_balance_field(Result(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs object(2,0) 200 100 @A
//> 0: ex::m::claim(Input(0));
//> ex::m::assert_funds(Result(0), Input(1), Input(2));
//> TransferObjects([Result(0)], Input(3))

// The parent is a dynamic object field of a root object, so the inherited version is the root's.
//# run ex::m::create_root --sender A

//# programmable --sender A --inputs object(11,0) 1000 @A
//> 0: ex::m::claim_from_root(Input(0));
//> 1: SplitCoins(Gas, [Input(1)]);
//> ex::m::fund(Result(0), Result(1));
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(12,1) 400 400 @A
//> 0: ex::m::withdraw(Input(0), Input(1));
//> 1: ex::m::withdraw_coin(Input(0), Input(2));
//> TransferObjects([Result(0), Result(1)], Input(3))

//# programmable --sender A --inputs object(11,0) object(12,1)
//> ex::m::release_from_root(Input(0), Input(1))

//# programmable --sender A --inputs object(11,0) 101 @A
//> 0: ex::m::claim_from_root(Input(0));
//> 1: ex::m::withdraw(Result(0), Input(1));
//> TransferObjects([Result(0), Result(1)], Input(2))

//# programmable --sender A --inputs object(11,0) 100 100 @A
//> 0: ex::m::claim_from_root(Input(0));
//> ex::m::assert_funds(Result(0), Input(1), Input(2));
//> TransferObjects([Result(0)], Input(3))

// The parent is a bare UID wrapped in an input object.
//# run ex::m::create_holder --sender A

//# programmable --sender A --inputs object(17,0) 1000 @A
//> 0: ex::m::claim_from_holder(Input(0));
//> 1: SplitCoins(Gas, [Input(1)]);
//> ex::m::fund(Result(0), Result(1));
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(18,1) 400 400 @A
//> 0: ex::m::withdraw(Input(0), Input(1));
//> 1: ex::m::withdraw_coin(Input(0), Input(2));
//> TransferObjects([Result(0), Result(1)], Input(3))

//# programmable --sender A --inputs object(17,0) object(18,1)
//> ex::m::release_from_holder(Input(0), Input(1))

//# programmable --sender A --inputs object(17,0) 101 @A
//> 0: ex::m::claim_from_holder(Input(0));
//> 1: ex::m::withdraw(Result(0), Input(1));
//> TransferObjects([Result(0), Result(1)], Input(2))

//# programmable --sender A --inputs object(17,0) 100 100 @A
//> 0: ex::m::claim_from_holder(Input(0));
//> ex::m::assert_funds(Result(0), Input(1), Input(2));
//> TransferObjects([Result(0)], Input(3))

