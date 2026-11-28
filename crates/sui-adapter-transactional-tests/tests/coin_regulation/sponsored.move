// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A frozen sender and a frozen gas owner are each denied in a sponsored transaction. A failing
// transaction that involves a frozen address reports its own error. The coin sits in a shared
// pool's dynamic field, so only execution sees its type.

//# init --accounts A B C --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::coin::Coin;
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

public struct Pool has key {
    id: UID,
}

public fun share(coin: Coin<REGULATED_COIN>, ctx: &mut TxContext) {
    let mut id = object::new(ctx);
    dynamic_field::add(&mut id, 0u8, coin);
    transfer::share_object(Pool { id });
}

public fun take(pool: &mut Pool, amount: u64, recipient: address, ctx: &mut TxContext) {
    let coin: &mut Coin<REGULATED_COIN> = dynamic_field::borrow_mut(&mut pool.id, 0u8);
    transfer::public_transfer(coin.split(amount, ctx), recipient);
}

public fun take_and_abort(pool: &mut Pool, amount: u64, recipient: address, ctx: &mut TxContext) {
    take(pool, amount, recipient, ctx);
    abort 42
}

//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(1,5) 100
//> 0: SplitCoins(Input(0), [Input(1)]);
//> test::regulated_coin::share(Result(0))

// B gets a SUI address balance to pay gas from.
//# programmable --sender A --inputs 10000000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1))

//# create-checkpoint

// Control: C takes from the pool, sponsored by B.
//# programmable --sender C --sponsor B --inputs object(3,0) 1 @C
//> test::regulated_coin::take(Input(0), Input(1), Input(2))

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// The frozen sender B, sponsored by A.
//# programmable --sender B --sponsor A --inputs object(3,0) 1 @C
//> test::regulated_coin::take(Input(0), Input(1), Input(2))

// C, sponsored by the frozen B with a gas coin.
//# programmable --sender C --sponsor B --inputs object(3,0) 1 @C
//> test::regulated_coin::take(Input(0), Input(1), Input(2))

// C, sponsored by the frozen B from its address balance.
//# programmable --sender C --sponsor B --address-balance-gas --inputs object(3,0) 1 @C
//> test::regulated_coin::take(Input(0), Input(1), Input(2))

// Control: C, sponsored by A.
//# programmable --sender C --sponsor A --inputs object(3,0) 1 @C
//> test::regulated_coin::take(Input(0), Input(1), Input(2))

// The frozen sender's transaction aborts, and reports the abort.
//# programmable --sender B --sponsor A --inputs object(3,0) 1 @C
//> test::regulated_coin::take_and_abort(Input(0), Input(1), Input(2))
