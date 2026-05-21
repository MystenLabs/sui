// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines its own coin type and exposes monomorphic entry functions for
/// mint/burn/split/merge so the surfer exercises the `coin`/`balance` machinery
/// (which is otherwise only reachable via generic framework calls).
module move_building_blocks::coins {
    use sui::coin::{Self, Coin, TreasuryCap};

    /// One-time witness for the coin type.
    public struct COINS has drop {}

    /// Shared holder for the treasury cap so any account can mint.
    public struct Treasury has key {
        id: UID,
        cap: TreasuryCap<COINS>,
    }

    fun init(witness: COINS, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency(
            witness,
            9,
            b"SURF",
            b"Surfer Coin",
            b"Coin minted by sui-surfer building blocks",
            option::none(),
            ctx,
        );
        transfer::public_freeze_object(metadata);
        transfer::share_object(Treasury { id: object::new(ctx), cap: treasury_cap });
    }

    public fun mint(treasury: &mut Treasury, amount: u64, ctx: &mut TxContext) {
        let coin = coin::mint(&mut treasury.cap, amount % 1_000_000, ctx);
        transfer::public_transfer(coin, ctx.sender());
    }

    public fun burn(treasury: &mut Treasury, coin: Coin<COINS>) {
        let _ = coin::burn(&mut treasury.cap, coin);
    }

    public fun split(coin: &mut Coin<COINS>, amount: u64, ctx: &mut TxContext) {
        let value = coin.value();
        if (value > 1) {
            let split_amount = 1 + (amount % (value - 1));
            let new_coin = coin.split(split_amount, ctx);
            transfer::public_transfer(new_coin, ctx.sender());
        }
    }

    public fun merge(coin: &mut Coin<COINS>, other: Coin<COINS>) {
        coin.join(other);
    }

    public fun zero_and_destroy(ctx: &mut TxContext) {
        let coin = coin::zero<COINS>(ctx);
        coin.destroy_zero();
    }
}
