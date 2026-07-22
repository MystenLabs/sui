// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A module to test coin index.
module fungible_tokens::managed;

use sui::coin::{Self, Coin, TreasuryCap};
use sui::table_vec::{Self, TableVec};

public struct PublicRedEnvelope has key, store {
    id: UID,
    coins: TableVec<Coin<MANAGED>>,
}

/// Name of the coin. By convention, this type has the same name as its parent module
/// and has no fields. The full type of the coin defined by this module will be `COIN<MANAGED>`.
public struct MANAGED has drop {}

/// Register the managed currency to acquire its `TreasuryCap`. Because
/// this is a module initializer, it ensures the currency only gets
/// registered once.
fun init(witness: MANAGED, ctx: &mut TxContext) {
    // Get a treasury cap for the coin and give it to the transaction sender
    let (treasury_cap, metadata) = coin::create_currency<MANAGED>(
        witness,
        2,
        b"MANAGED",
        b"",
        b"",
        option::none(),
        ctx,
    );
    transfer::public_freeze_object(metadata);
    transfer::public_transfer(treasury_cap, ctx.sender());

    let red_envelopes = PublicRedEnvelope { id: object::new(ctx), coins: table_vec::empty(ctx) };
    transfer::share_object(red_envelopes)
}

public fun mint(
    treasury_cap: &mut TreasuryCap<MANAGED>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    treasury_cap.mint_and_transfer(amount, recipient, ctx)
}

public fun mint_multi(
    treasury_cap: &mut TreasuryCap<MANAGED>,
    amount: u64,
    num: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    let mut i = 0;
    while (i < num) {
        treasury_cap.mint_and_transfer(amount, recipient, ctx);
        i = i + 1;
    }
}

public fun add_to_envelope(red_envelopes: &mut PublicRedEnvelope, coin: Coin<MANAGED>) {
    red_envelopes.coins.push_back(coin)
}

public fun take_from_envelope(red_envelopes: &mut PublicRedEnvelope, ctx: &mut TxContext) {
    let coin = red_envelopes.coins.pop_back();
    transfer::public_transfer(coin, ctx.sender())
}

public fun take_from_envelope_and_burn(
    treasury_cap: &mut TreasuryCap<MANAGED>,
    red_envelopes: &mut PublicRedEnvelope,
) {
    let coin = red_envelopes.coins.pop_back();
    treasury_cap.burn(coin);
}

public fun burn(treasury_cap: &mut TreasuryCap<MANAGED>, coin: Coin<MANAGED>) {
    treasury_cap.burn(coin);
}
