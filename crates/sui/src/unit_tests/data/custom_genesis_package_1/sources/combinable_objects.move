// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example of objects that can be combined to create
/// new objects
module examples::combinable_objects {
    use examples::trusted_coin::EXAMPLE;
    use sui::coin::{Self, Coin};
    use sui::id::{Self, VersionedID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct Ham has key {
        id: VersionedID
    }

    struct Bread has key {
        id: VersionedID
    }

    struct Sandwich has key {
        id: VersionedID
    }

    /// Address selling ham, bread, etc
    const GROCERY: address = @0x0;
    /// Price for ham
    const HAM_PRICE: u64 = 10;
    /// Price for bread
    const BREAD_PRICE: u64 = 2;

    /// Not enough funds to pay for the good in question
    const EINSUFFICIENT_FUNDS: u64 = 0;

    /// Exchange `c` for some ham
    public fun buy_ham(c: Coin<EXAMPLE>, ctx: &mut TxContext): Ham {
        assert!(coin::value(&c) == HAM_PRICE, EINSUFFICIENT_FUNDS);
        transfer::transfer(c, admin());
        Ham { id: tx_context::new_id(ctx) }
    }

    /// Exchange `c` for some bread
    public fun buy_bread(c: Coin<EXAMPLE>, ctx: &mut TxContext): Bread {
        assert!(coin::value(&c) == BREAD_PRICE, EINSUFFICIENT_FUNDS);
        transfer::transfer(c, admin());
        Bread { id: tx_context::new_id(ctx) }
    }

    /// Combine the `ham` and `bread` into a delicious sandwich
    public fun make_sandwich(
        ham: Ham, bread: Bread, ctx: &mut TxContext
    ): Sandwich {
        let Ham { id: ham_id } = ham;
        let Bread { id: bread_id } = bread;
        id::delete(ham_id);
        id::delete(bread_id);
        Sandwich { id: tx_context::new_id(ctx) }
    }

    fun admin(): address {
        GROCERY
    }
}
