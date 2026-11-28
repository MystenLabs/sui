// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A coin type first defined in version 2 of its package.
// Later transactions name the coin type as V2::m::New.
// Named by its defining IDs it is V2::m::New.
// `Old`, `One`, and `Wrapper` are defined in version 1, `New` in version 2. B is frozen, then the
// coin is paused. Every transfer that involves B, and every transfer after the pause, is denied.

//# init --accounts A B C --addresses V1=0x0 V2=0x0 V3=0x0

//# publish --upgradeable --sender A
#[allow(unused)]
module V1::m {
    use std::ascii::String;
    use std::type_name;
    use sui::balance::Balance;
    use sui::coin::{Coin, TreasuryCap};
    use sui::coin_registry::{Self, CoinRegistry, CurrencyInitializer};
    use sui::dynamic_field as df;

    public struct Old has key { id: UID }
    public struct One<phantom A> has key { id: UID }

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

    public fun create_old(registry: &mut CoinRegistry, b: address, ctx: &mut TxContext) {
        let (currency, treasury_cap) = coin_registry::new_currency<Old>(
            registry,
            9,
            b"UP".to_string(),
            b"Upgrade".to_string(),
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
            b"UP".to_string(),
            b"Upgrade".to_string(),
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
        transfer::transfer(
            Wrapper { id: object::new(ctx), coin: coin.split(10, ctx) },
            ctx.sender(),
        );
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
}

//# upgrade --package V1 --upgrade-capability 1,1 --sender A
#[allow(unused)]
module V2::m {
    use std::ascii::String;
    use std::type_name;
    use sui::balance::Balance;
    use sui::coin::{Coin, TreasuryCap};
    use sui::coin_registry::{Self, CoinRegistry, CurrencyInitializer};
    use sui::dynamic_field as df;

    public struct Old has key { id: UID }
    public struct One<phantom A> has key { id: UID }

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

    public fun create_old(registry: &mut CoinRegistry, b: address, ctx: &mut TxContext) {
        let (currency, treasury_cap) = coin_registry::new_currency<Old>(
            registry,
            9,
            b"UP".to_string(),
            b"Upgrade".to_string(),
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
            b"UP".to_string(),
            b"Upgrade".to_string(),
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
        transfer::transfer(
            Wrapper { id: object::new(ctx), coin: coin.split(10, ctx) },
            ctx.sender(),
        );
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

    // First defined in version 2.
    public struct New has key { id: UID }

    public fun create_new(registry: &mut CoinRegistry, b: address, ctx: &mut TxContext) {
        let (currency, treasury_cap) = coin_registry::new_currency<New>(
            registry,
            9,
            b"UP".to_string(),
            b"Upgrade".to_string(),
            b"".to_string(),
            b"".to_string(),
            ctx,
        );
        finish(currency, treasury_cap, b, ctx);
    }
}

//# upgrade --package V2 --upgrade-capability 1,1 --sender A
#[allow(unused)]
module V3::m {
    use std::ascii::String;
    use std::type_name;
    use sui::balance::Balance;
    use sui::coin::{Coin, TreasuryCap};
    use sui::coin_registry::{Self, CoinRegistry, CurrencyInitializer};
    use sui::dynamic_field as df;

    public struct Old has key { id: UID }
    public struct One<phantom A> has key { id: UID }

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

    public fun create_old(registry: &mut CoinRegistry, b: address, ctx: &mut TxContext) {
        let (currency, treasury_cap) = coin_registry::new_currency<Old>(
            registry,
            9,
            b"UP".to_string(),
            b"Upgrade".to_string(),
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
            b"UP".to_string(),
            b"Upgrade".to_string(),
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
        transfer::transfer(
            Wrapper { id: object::new(ctx), coin: coin.split(10, ctx) },
            ctx.sender(),
        );
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

    // First defined in version 2.
    public struct New has key { id: UID }

    public fun create_new(registry: &mut CoinRegistry, b: address, ctx: &mut TxContext) {
        let (currency, treasury_cap) = coin_registry::new_currency<New>(
            registry,
            9,
            b"UP".to_string(),
            b"Upgrade".to_string(),
            b"".to_string(),
            b"".to_string(),
            ctx,
        );
        finish(currency, treasury_cap, b, ctx);
    }

    public fun version(): u64 { 3 }
}

//# run V2::m::create_new --args object(0xc) @B --sender A

// `type_name::with_defining_ids<T>()` as Move prints it.
//# view-object 4,7

// A's Coin.
//# view-object 4,3

// B's Wrapper.
//# view-object 4,5

//# create-checkpoint

// Control before the freeze: B can send.
//# programmable --sender B --inputs object(4,4) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(4,1) @B --type-args V2::m::New --sender A

// The `CoinTypeKey` field.
//# view-object 10,0

// The `FrozenAddressKey` field.
//# view-object 10,1

// Frozen sender: Coin.
//# programmable --sender B --inputs object(4,4) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Frozen sender: Coin held in an object.
//# run V3::m::split_wrapped --args object(4,5) 1 @A --type-args V2::m::New --sender B

// Frozen sender: address balance, with the type named as in the freeze.
//# programmable --sender B --inputs withdraw<sui::balance::Balance<V2::m::New>>(1) @A
//> 0: sui::balance::redeem_funds<V2::m::New>(Input(0));
//> 1: sui::balance::send_funds<V2::m::New>(Result(0), Input(1));

// Frozen sender: address balance, with the type named by its defining IDs.
//# programmable --sender B --inputs withdraw<sui::balance::Balance<V2::m::New>>(1) @A
//> 0: sui::balance::redeem_funds<V2::m::New>(Input(0));
//> 1: sui::balance::send_funds<V2::m::New>(Result(0), Input(1));

// Frozen sender: address balance funded from a Coin held in an object.
//# run V3::m::send_wrapped_funds --args object(4,5) 1 @A --type-args V2::m::New --sender B

// Frozen recipient: Coin.
//# programmable --sender A --inputs object(4,3) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Frozen recipient: address balance.
//# programmable --sender A --inputs object(4,3) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> sui::coin::send_funds<V2::m::New>(Result(0), Input(2))

// Frozen recipient: an object that holds the Coin.
//# run V3::m::give_wrapped --args object(4,6) @B --type-args V2::m::New --sender A

// Frozen sender: reads a Balance held in a dynamic field.
//# run V3::m::peek_boxed --args object(4,8) --type-args V2::m::New --sender B

// Frozen sender: moves a Balance between dynamic fields.
//# run V3::m::rebox --args object(4,8) --type-args V2::m::New --sender B

// Control: an unfrozen recipient is allowed.
//# programmable --sender A --inputs object(4,3) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# run sui::coin_regulation::enable_global_pause --args object(0xcf) object(4,1) --type-args V2::m::New --sender A

// Paused: Coin input, no frozen address involved.
//# programmable --sender A --inputs object(4,3) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Paused: Coin held in an object, no frozen address involved.
//# run V3::m::split_wrapped --args object(4,6) 1 @C --type-args V2::m::New --sender A

// Paused: address balance deposit from a Coin held in an object.
//# run V3::m::send_wrapped_funds --args object(4,6) 1 @C --type-args V2::m::New --sender A

// Paused: an object that holds the Coin changes owner.
//# run V3::m::give_wrapped --args object(4,6) @C --type-args V2::m::New --sender A

// Paused: reads a Balance held in a dynamic field.
//# run V3::m::peek_boxed --args object(4,9) --type-args V2::m::New --sender A

// Paused: moves a Balance between dynamic fields.
//# run V3::m::rebox --args object(4,9) --type-args V2::m::New --sender A
