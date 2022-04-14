// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example of objects that can be combined to create
/// new objects
module Basics::Sandwich {
    use Sui::Coin::{Self, Coin};
    use Sui::ID::{Self, VersionedID};
    use Sui::SUI::SUI;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    struct Ham has key {
        id: VersionedID
    }

    struct Bread has key {
        id: VersionedID
    }

    struct Sandwich has key {
        id: VersionedID,
    }

    // This Capability allows the owner to withdraw profits
    struct GroceryOwnerCapability has key {}

    // Grocery is created on module init
    struct Grocery has key {
        id: VersionedID,
        profits: Coin<SUI>
    }

    /// Price for ham
    const HAM_PRICE: u64 = 10;
    /// Price for bread
    const BREAD_PRICE: u64 = 2;

    /// Not enough funds to pay for the good in question
    const EINSUFFICIENT_FUNDS: u64 = 0;

    /// On module init, create a grocery
    fun init(ctx: &mut TxContext) {
        Transfer::share_object(Grocery { 
            id: TxContext::new_id(ctx),
            profits: Coin::zero<SUI>()
        });

        Transfer::transfer(GroceryOwnerCapability, TxContext::sender(ctx));
    }

    /// Exchange `c` for some ham
    public fun buy_ham(grocery: &mut Grocery, c: Coin<SUI>, ctx: &mut TxContext): Ham {
        assert!(Coin::value(&c) == HAM_PRICE, EINSUFFICIENT_FUNDS);
        Coin::join(&mut grocery.profits, c);
        Ham { id: TxContext::new_id(ctx) }
    }

    /// Exchange `c` for some bread
    public fun buy_bread(grocery: &mut Grocery, c: Coin<SUI>, ctx: &mut TxContext): Bread {
        assert!(Coin::value(&c) == BREAD_PRICE, EINSUFFICIENT_FUNDS);
        Coin::join(&mut grocery.profits, c);
        Bread { id: TxContext::new_id(ctx) }
    }

    /// Combine the `ham` and `bread` into a delicious sandwich
    public fun make_sandwich(ham: Ham, bread: Bread, ctx: &mut TxContext) {
        let Ham { id: ham_id } = ham;
        let Bread { id: bread_id } = bread;
        ID::delete(ham_id);
        ID::delete(bread_id);
        Transfer::transfer(Sandwich { id: TxContext::new_id(ctx) }, TxContext::sender(ctx))
    }

    /// Owner of the grocery can collect profits by passing his capability
    public fun collect_profits(cap: GroceryOwnerCapability, grocery: &mut Grocery, ctx: &mut TxContext) {
        let coin = Coin::withdraw(&mut grocery.profits, Coin::amount(&grocery.profits));
        Transfer::transfer(coin, TxContext::sender(ctx));
        Transfer::transfer(cap, TxContext::sender(ctx));
    }
}
