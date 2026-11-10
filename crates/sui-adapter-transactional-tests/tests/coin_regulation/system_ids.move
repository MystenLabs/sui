// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Freezing the Clock, the framework package, or the coin's own package blocks nothing.

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::clock::Clock;
use sui::coin::Coin;
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
    let cap = currency.make_compliance_regulated(false, ctx);
    let metadata_cap = currency.finalize(ctx);
    let coin = treasury_cap.mint(10000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(cap, ctx.sender());
    transfer::public_transfer(metadata_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
}

public fun pay(
    coin: &mut Coin<REGULATED_COIN>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    transfer::public_transfer(coin.split(amount, ctx), recipient);
}

public fun pay_at(
    coin: &mut Coin<REGULATED_COIN>,
    amount: u64,
    recipient: address,
    clock: &Clock,
    ctx: &mut TxContext,
) {
    clock.timestamp_ms();
    transfer::public_transfer(coin.split(amount, ctx), recipient);
}

public fun pay_generic<T, U>(
    coin: &mut Coin<T>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    transfer::public_transfer(coin.split(amount, ctx), recipient);
}

public fun read_clock(clock: &Clock): u64 {
    clock.timestamp_ms()
}

public fun send_all(coins: vector<Coin<REGULATED_COIN>>, recipient: address) {
    coins.do!(|coin| transfer::public_transfer(coin, recipient));
}

//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

// The cap cannot pause.
//# run sui::coin_regulation::enable_global_pause --args object(0xcf) object(1,3) --type-args test::regulated_coin::REGULATED_COIN --sender A

// B gets an address balance and a Coin.
//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1), Input(1)]);
//> 1: sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(NestedResult(0,0), Input(2));
//> TransferObjects([NestedResult(0,1)], Input(2))

//# create-checkpoint

// Before any freeze: a call into 0x2 with the coin as a type argument.
//# programmable --sender A --inputs object(1,5) 1 @B
//> sui::pay::split_and_transfer<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2))

// Before any freeze: a call that reads the Clock.
//# programmable --sender A --inputs object(1,5) 1 @B immshared(6)
//> test::regulated_coin::pay_at(Input(0), Input(1), Input(2), Input(3))

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @0x6 --type-args test::regulated_coin::REGULATED_COIN --sender A

// Clock frozen: a call that reads the Clock and moves the coin.
//# programmable --sender A --inputs object(1,5) 1 @B immshared(6)
//> test::regulated_coin::pay_at(Input(0), Input(1), Input(2), Input(3))

// Clock frozen: the Clock is read by one command and the coin is moved by other commands.
//# programmable --sender A --inputs immshared(6) object(1,5) 1 @B
//> 0: test::regulated_coin::read_clock(Input(0));
//> 1: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(1)], Input(3))

// Clock frozen: a Clock read without the coin.
//# programmable --sender A --inputs immshared(6)
//> test::regulated_coin::read_clock(Input(0))

// Clock frozen: a call into 0x2 with the coin as a type argument and no Clock.
//# programmable --sender A --inputs object(1,5) 1 @B
//> sui::pay::split_and_transfer<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2))

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @0x2 --type-args test::regulated_coin::REGULATED_COIN --sender A

// 0x2 frozen: a call into 0x2 with the coin as a type argument.
//# programmable --sender A --inputs object(1,5) 1 @B
//> sui::pay::split_and_transfer<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2))

// 0x2 frozen: a deposit to an address balance.
//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> sui::coin::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(2))

// 0x2 frozen: a withdrawal from an address balance.
//# programmable --sender B --inputs withdraw<sui::balance::Balance<test::regulated_coin::REGULATED_COIN>>(1) @A
//> 0: sui::balance::redeem_funds<test::regulated_coin::REGULATED_COIN>(Input(0));
//> 1: sui::balance::send_funds<test::regulated_coin::REGULATED_COIN>(Result(0), Input(1));

// 0x2 frozen: MakeMoveVec with a 0x2 type, and a call into the coin's package.
//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MakeMoveVec<sui::coin::Coin<test::regulated_coin::REGULATED_COIN>>([Result(0)]);
//> test::regulated_coin::send_all(Result(1), Input(2))

// 0x2 frozen: a call into the coin's package with no type argument.
//# programmable --sender A --inputs object(1,5) 1 @B
//> test::regulated_coin::pay(Input(0), Input(1), Input(2))

// 0x2 frozen: a call into the coin's package with the coin as the only type argument.
//# programmable --sender A --inputs object(1,5) 1 @B
//> test::regulated_coin::pay_generic<test::regulated_coin::REGULATED_COIN, test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2))

// 0x2 frozen: the same call with SUI as an unused type argument.
//# programmable --sender A --inputs object(1,5) 1 @B
//> test::regulated_coin::pay_generic<test::regulated_coin::REGULATED_COIN, sui::sui::SUI>(Input(0), Input(1), Input(2))

// 0x2 frozen: plain SplitCoins and TransferObjects.
//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// 0x2 frozen: plain TransferObjects of a whole Coin.
//# programmable --sender B --inputs object(4,0) @A
//> TransferObjects([Input(0)], Input(1))

// 0x2 frozen: plain MergeCoins.
//# programmable --sender A --inputs object(1,5) object(4,0)
//> MergeCoins(Input(0), [Input(1)])

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @test --type-args test::regulated_coin::REGULATED_COIN --sender A

// Coin package frozen: a call into the coin's package with no type argument.
//# programmable --sender A --inputs object(1,5) 1 @B
//> test::regulated_coin::pay(Input(0), Input(1), Input(2))

// Coin package frozen: plain SplitCoins and TransferObjects.
//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// The unfreeze is itself a call into 0x2 with the coin as a type argument.
//# run sui::coin_regulation::unfreeze_address --args object(0xcf) object(1,3) @0x2 --type-args test::regulated_coin::REGULATED_COIN --sender A

// Only the coin package and the Clock frozen: a call into 0x2 with the coin as a type argument.
//# programmable --sender A --inputs object(1,5) 1 @B
//> sui::pay::split_and_transfer<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2))

//# run sui::coin_regulation::unfreeze_address --args object(0xcf) object(1,3) @test --type-args test::regulated_coin::REGULATED_COIN --sender A

// Only the Clock frozen again.
//# programmable --sender A --inputs object(1,5) 1 @B
//> sui::pay::split_and_transfer<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2))
