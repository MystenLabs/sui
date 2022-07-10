// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the Hero object with a Bag. Bag can be used to hold
/// (be parent to) objects of any type, allowing rich usecases
/// in other applications.
module 0x0::hero {
    use sui::id::VersionedID;
    use sui::bag::{Self, Bag};
    use sui::utf8::{Self, String};
    use sui::transfer::{Self, ChildRef};
    use sui::tx_context::{Self, TxContext};

    /// Object representing our Hero.
    struct Hero has key {
        id: VersionedID,
        name: String,
        backpack: ChildRef<Bag>
    }

    /// Create new hero and transfer it to sender.
    entry fun create_hero(name: vector<u8>, ctx: &mut TxContext) {
        // Bag initializer requires `VersionedID` as an argument which
        // it returns in a return tuple.
        // Type signature here is: (VersionedID, ChildRef<Bag>)
        let (id, backpack) = bag::transfer_to_object_id(
            bag::new(ctx),
            tx_context::new_id(ctx)
        );

        transfer::transfer(Hero {
            id,
            backpack,
            name: utf8::string_unsafe(name),
        }, tx_context::sender(ctx))
    }

    /// Add a new item to the backpack. Keeping it generic so any
    /// application can create and add items to this hero's backpack.
    ///
    /// To use a Bag, sender has to have access to the Hero object (by
    /// either owning it or if it is shared).
    public entry fun add_to_backpack<T: key + store>(
        _: &mut Hero,
        backpack: &mut Bag,
        item: T
    ) {
        bag::add(backpack, item)
    }
}

/// Another application that makes use of Hero.
/// It could be extended to support payments and/or some logic.
module 0x0::arena {
    use sui::bag::Bag;
    use sui::id::VersionedID;
    use sui::tx_context::{Self, TxContext};

    // Importing the Hero module to reuse its types.
    use 0x0::hero::{Self, Hero};

    /// A freely-mintable sword.
    struct Sword has key, store {
        id: VersionedID,
        power: u64
    }

    /// Create a new Sword and add it to the backpack.
    public entry fun create_sword_for_hero(
        hero: &mut Hero,
        backpack: &mut Bag,
        ctx: &mut TxContext
    ) {
        hero::add_to_backpack(hero, backpack, Sword {
            id: tx_context::new_id(ctx),
            power: 1000
        });
    }
}
