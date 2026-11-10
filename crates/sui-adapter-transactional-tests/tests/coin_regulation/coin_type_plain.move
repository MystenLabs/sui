// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Coin type `test::shapes::Plain`, made with `coin_registry::new_currency`.
// B is frozen, then the coin is paused. Every transfer that involves B, and every transfer after
// the pause, is denied.

//# init --accounts A B C --addresses test=0x0

//# publish --sender A
#[allow(unused)]
module test::shapes;

use std::ascii::String;
use std::type_name;
use sui::balance::Balance;
use sui::coin::{Coin, TreasuryCap};
use sui::coin_registry::{Self, CoinRegistry, CurrencyInitializer};
use sui::dynamic_field as df;

public struct Plain has key { id: UID }
public struct One<phantom A> has key { id: UID }
public struct Two<phantom A, phantom B> has key { id: UID }
public struct Three<phantom A, phantom B, phantom C> has key { id: UID }
public struct Tag<phantom A> {}

public struct Wrapper<phantom T> has key {
    id: UID,
    coin: Coin<T>,
}

public struct Name has key {
    id: UID,
    name: String,
}

// Holds a `Balance<T>` in the dynamic field `0u64`, so the coin type is not in its own type.
public struct Box has key { id: UID }

public fun create_plain(registry: &mut CoinRegistry, b: address, ctx: &mut TxContext) {
    let (currency, treasury_cap) = coin_registry::new_currency<Plain>(
        registry,
        9,
        b"SH".to_string(),
        b"Shape".to_string(),
        b"".to_string(),
        b"".to_string(),
        ctx,
    );
    finish(currency, treasury_cap, b, ctx);
}

public fun create_one<A>(registry: &mut CoinRegistry, b: address, ctx: &mut TxContext) {
    let (currency, treasury_cap) = coin_registry::new_currency<One<A>>(
        registry,
        9,
        b"SH".to_string(),
        b"Shape".to_string(),
        b"".to_string(),
        b"".to_string(),
        ctx,
    );
    finish(currency, treasury_cap, b, ctx);
}

public fun create_two<A, B>(registry: &mut CoinRegistry, b: address, ctx: &mut TxContext) {
    let (currency, treasury_cap) = coin_registry::new_currency<Two<A, B>>(
        registry,
        9,
        b"SH".to_string(),
        b"Shape".to_string(),
        b"".to_string(),
        b"".to_string(),
        ctx,
    );
    finish(currency, treasury_cap, b, ctx);
}

public fun create_three<A, B, C>(registry: &mut CoinRegistry, b: address, ctx: &mut TxContext) {
    let (currency, treasury_cap) = coin_registry::new_currency<Three<A, B, C>>(
        registry,
        9,
        b"SH".to_string(),
        b"Shape".to_string(),
        b"".to_string(),
        b"".to_string(),
        ctx,
    );
    finish(currency, treasury_cap, b, ctx);
}

// Fresh IDs in order: TreasuryCap, ComplianceCap, MetadataCap, A's Coin, B's Coin, B's Wrapper,
// A's Wrapper, Name, B's Box, A's Box.
fun finish<T>(
    mut currency: CurrencyInitializer<T>,
    mut treasury_cap: TreasuryCap<T>,
    b: address,
    ctx: &mut TxContext,
) {
    let cap = currency.make_compliance_regulated(true, ctx);
    let metadata_cap = currency.finalize(ctx);
    let mut coin = treasury_cap.mint(10000, ctx);
    transfer::public_transfer(coin.split(100, ctx), b);
    coin.split(100, ctx).send_funds(b);
    transfer::transfer(Wrapper { id: object::new(ctx), coin: coin.split(10, ctx) }, b);
    transfer::transfer(Wrapper { id: object::new(ctx), coin: coin.split(10, ctx) }, ctx.sender());
    transfer::transfer(
        Name { id: object::new(ctx), name: type_name::with_defining_ids<T>().into_string() },
        ctx.sender(),
    );
    let mut box = Box { id: object::new(ctx) };
    df::add(&mut box.id, 0u64, coin.split(10, ctx).into_balance());
    transfer::transfer(box, b);
    let mut box = Box { id: object::new(ctx) };
    df::add(&mut box.id, 0u64, coin.split(10, ctx).into_balance());
    transfer::transfer(box, ctx.sender());
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(cap, ctx.sender());
    transfer::public_transfer(metadata_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
}

public fun split_wrapped<T>(
    wrapper: &mut Wrapper<T>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    transfer::public_transfer(wrapper.coin.split(amount, ctx), recipient);
}

public fun send_wrapped_funds<T>(
    wrapper: &mut Wrapper<T>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    wrapper.coin.split(amount, ctx).send_funds(recipient);
}

public fun give_wrapped<T>(wrapper: Wrapper<T>, recipient: address) {
    transfer::transfer(wrapper, recipient);
}

public fun peek_boxed<T>(box: &Box): u64 {
    df::borrow<u64, Balance<T>>(&box.id, 0).value()
}

public fun rebox<T>(box: &mut Box) {
    let balance: Balance<T> = df::remove(&mut box.id, 0u64);
    df::add(&mut box.id, 1u64, balance);
}

//# run test::shapes::create_plain --args object(0xc) @B --sender A

// `type_name::with_defining_ids<T>()` as Move prints it.
//# view-object 2,7

// A's Coin.
//# view-object 2,3

// B's Coin.
//# view-object 2,4

// B's Wrapper.
//# view-object 2,5

//# create-checkpoint

// Control before the freeze: B can send.
//# programmable --sender B --inputs object(2,4) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(2,1) @B --type-args test::shapes::Plain --sender A

// The `CoinTypeKey` field.
//# view-object 9,0

// The `FrozenAddressKey` field.
//# view-object 9,1

// Frozen sender: Coin.
//# programmable --sender B --inputs object(2,4) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Frozen sender: Coin held in an object.
//# run test::shapes::split_wrapped --args object(2,5) 1 @A --type-args test::shapes::Plain --sender B

// Frozen sender: address balance.
//# programmable --sender B --inputs withdraw<sui::balance::Balance<test::shapes::Plain>>(1) @A
//> 0: sui::balance::redeem_funds<test::shapes::Plain>(Input(0));
//> 1: sui::balance::send_funds<test::shapes::Plain>(Result(0), Input(1));

// Frozen sender: address balance funded from a Coin held in an object.
//# run test::shapes::send_wrapped_funds --args object(2,5) 1 @A --type-args test::shapes::Plain --sender B

// Frozen recipient: Coin.
//# programmable --sender A --inputs object(2,3) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Frozen recipient: address balance.
//# programmable --sender A --inputs object(2,3) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> sui::coin::send_funds<test::shapes::Plain>(Result(0), Input(2))

// Frozen recipient: an object that holds the Coin.
//# run test::shapes::give_wrapped --args object(2,6) @B --type-args test::shapes::Plain --sender A

// Frozen sender: reads a Balance held in a dynamic field.
//# run test::shapes::peek_boxed --args object(2,8) --type-args test::shapes::Plain --sender B

// Frozen sender: moves a Balance between dynamic fields.
//# run test::shapes::rebox --args object(2,8) --type-args test::shapes::Plain --sender B

// Control: an unfrozen recipient is allowed.
//# programmable --sender A --inputs object(2,3) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# run sui::coin_regulation::enable_global_pause --args object(0xcf) object(2,1) --type-args test::shapes::Plain --sender A

// Paused: Coin input, no frozen address involved.
//# programmable --sender A --inputs object(2,3) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Paused: Coin held in an object, no frozen address involved.
//# run test::shapes::split_wrapped --args object(2,6) 1 @C --type-args test::shapes::Plain --sender A

// Paused: address balance deposit from a Coin held in an object.
//# run test::shapes::send_wrapped_funds --args object(2,6) 1 @C --type-args test::shapes::Plain --sender A

// Paused: an object that holds the Coin changes owner.
//# run test::shapes::give_wrapped --args object(2,6) @C --type-args test::shapes::Plain --sender A

// Paused: reads a Balance held in a dynamic field.
//# run test::shapes::peek_boxed --args object(2,9) --type-args test::shapes::Plain --sender A

// Paused: moves a Balance between dynamic fields.
//# run test::shapes::rebox --args object(2,9) --type-args test::shapes::Plain --sender A
