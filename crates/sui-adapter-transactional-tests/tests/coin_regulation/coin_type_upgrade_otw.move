// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A one-time-witness coin made in version 1 of its package, before the upgrade.
// After the upgrade, every transaction names the coin type through the version 2 package ID.
// B is frozen, then the coin is paused. Every transfer that involves B, and every transfer after
// the pause, is denied.

//# init --accounts A B C --addresses V1=0x0 V2=0x0

//# publish --upgradeable --sender A
module V1::regulated_coin {
    use sui::coin;
    use sui::coin_registry;

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
        coin: coin::Coin<REGULATED_COIN>,
    }

    public fun wrap(coin: coin::Coin<REGULATED_COIN>, recipient: address, ctx: &mut TxContext) {
        transfer::transfer(Wrapper { id: object::new(ctx), coin }, recipient);
    }

    public fun split_wrapped(
        wrapper: &mut Wrapper,
        amount: u64,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        transfer::public_transfer(wrapper.coin.split(amount, ctx), recipient);
    }

    public fun send_wrapped_funds(
        wrapper: &mut Wrapper,
        amount: u64,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        wrapper.coin.split(amount, ctx).send_funds(recipient);
    }
}

//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args V1::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# upgrade --package V1 --upgrade-capability 1,6 --sender A
module V2::regulated_coin {
    use sui::coin;
    use sui::coin_registry;

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
        coin: coin::Coin<REGULATED_COIN>,
    }

    public fun wrap(coin: coin::Coin<REGULATED_COIN>, recipient: address, ctx: &mut TxContext) {
        transfer::transfer(Wrapper { id: object::new(ctx), coin }, recipient);
    }

    public fun split_wrapped(
        wrapper: &mut Wrapper,
        amount: u64,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        transfer::public_transfer(wrapper.coin.split(amount, ctx), recipient);
    }

    public fun send_wrapped_funds(
        wrapper: &mut Wrapper,
        amount: u64,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        wrapper.coin.split(amount, ctx).send_funds(recipient);
    }

    public fun version(): u64 { 2 }
}

//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> sui::coin::send_funds<V2::regulated_coin::REGULATED_COIN>(Result(0), Input(2))

//# programmable --sender B --inputs object(3,0) 10 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> V2::regulated_coin::wrap(Result(0), Input(2))

//# programmable --sender A --inputs object(1,5) 10 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> V2::regulated_coin::wrap(Result(0), Input(2))

//# create-checkpoint

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @B --type-args V2::regulated_coin::REGULATED_COIN --sender A

// The `CoinTypeKey` field.
//# view-object 9,0

// The `FrozenAddressKey` field.
//# view-object 9,1

// Frozen sender: Coin.
//# programmable --sender B --inputs object(3,0) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Frozen sender: Coin held in an object.
//# run V2::regulated_coin::split_wrapped --args object(6,0) 1 @A --sender B

// Frozen sender: address balance, with the type named through version 2.
//# programmable --sender B --inputs withdraw<sui::balance::Balance<V2::regulated_coin::REGULATED_COIN>>(1) @A
//> 0: sui::balance::redeem_funds<V2::regulated_coin::REGULATED_COIN>(Input(0));
//> 1: sui::balance::send_funds<V2::regulated_coin::REGULATED_COIN>(Result(0), Input(1));

// Frozen sender: address balance, with the type named by its defining ID.
//# programmable --sender B --inputs withdraw<sui::balance::Balance<V1::regulated_coin::REGULATED_COIN>>(1) @A
//> 0: sui::balance::redeem_funds<V1::regulated_coin::REGULATED_COIN>(Input(0));
//> 1: sui::balance::send_funds<V1::regulated_coin::REGULATED_COIN>(Result(0), Input(1));

// Frozen sender: address balance funded from a Coin held in an object.
//# run V2::regulated_coin::send_wrapped_funds --args object(6,0) 1 @A --sender B

// Frozen recipient: Coin.
//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Frozen recipient: address balance.
//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> sui::coin::send_funds<V2::regulated_coin::REGULATED_COIN>(Result(0), Input(2))

// Control: an unfrozen recipient is allowed.
//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# run sui::coin_regulation::enable_global_pause --args object(0xcf) object(1,3) --type-args V2::regulated_coin::REGULATED_COIN --sender A

// Paused: Coin input, no frozen address involved.
//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// Paused: Coin held in an object, no frozen address involved.
//# run V2::regulated_coin::split_wrapped --args object(7,0) 1 @C --sender A

// Paused: address balance deposit from a Coin held in an object.
//# run V2::regulated_coin::send_wrapped_funds --args object(7,0) 1 @C --sender A
