// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module presenting an implementable `Collectible` type.
/// It serves as a type-unifier and a generic interface for collectibles
/// by allowing type-param-specific implemntations for the common type.
///
/// Reference implementation can be found in tests.
///
/// TODO:
///  - should there be a `burn` method? does it decrease max_supply?
///  - example implementation in Sui Framework or Examples (DevNetNFT?)
module sui::collectible {
    use sui::tx_context::{TxContext};
    use std::option::{Self, Option};
    use sui::object::{Self, UID};
    use sui::transfer;

    /// For when owner tries to mint more than max_supply.
    const EMaxSupplyReached: u64 = 1;

    /// Generic type for collectibles.
    /// Combination of a `store` and `drop` capabilities saves
    /// from locking assets inside `Collectible` while making it possible
    /// to use `T` as both a witness and a data type.
    struct Collectible<T: store + drop> has key {
        id: UID,
        /// Incremental number in a collection. Useful for limited series
        /// In arts it's called `Edition` (eg 7/100)
        item_id: u64,
        /// Optional field for custom metadata.
        /// The same T is used for witness-ing transfer* methods.
        info: T,
    }

    /// A capability that allows the owner to mint new `Collectible` objects. Acts as
    /// a `TreasuryCap` for `Collectible`s.
    struct CollectionManagerCap<phantom T: store + drop> has key, store {
        id: UID,
        total_supply: u64,
        max_supply: Option<u64>
    }


    /// Borrow `item_id` field.
    public fun item_id<T: store + drop>(self: &Collectible<T>): u64 { self.item_id }

    /// Borrow `data` field.
    public fun info<T: store + drop>(self: &Collectible<T>): &T { &self.info }

    /// Implementable method giving a mutable reference to `data`.
    public fun info_mut<T: store + drop>(_w: T, self: &mut Collectible<T>): &mut T { &mut self.info }


    /// Create a Collection and receive a `CollectionManagerCap`. This object
    /// is similar to the `coin::TreasuryCap` in a way that it manages total
    /// supply and grants the owner permission to mint and burn new `Collectible`s.
    ///
    /// It also holds information about the `total_supply` and prevents from
    /// minting more than `max_supply`.
    public fun create_collection<T: store + drop>(
        _w: T, max_supply: Option<u64>, ctx: &mut TxContext
    ): CollectionManagerCap<T> {
        CollectionManagerCap {
            max_supply,
            id: object::new(ctx),
            total_supply: 0,
        }
    }

    /// Mint a new `Collectible` using `CollectionManagerCap`.
    public fun mint<T: store + drop>(
        cap: &mut CollectionManagerCap<T>, info: T, ctx: &mut TxContext
    ): Collectible<T> {
        cap.total_supply = cap.total_supply + 1;

        // if `max_supply` is set -> enforce it
        if (option::is_some(&cap.max_supply)) {
            assert!(cap.total_supply <= *option::borrow(&cap.max_supply), EMaxSupplyReached);
        };

        Collectible {
            info,
            id: object::new(ctx),
            item_id: cap.total_supply,
        }
    }


    /// Implementable `transfer::transfer` function.
    public fun transfer<T: store + drop>(_w: T, nft: Collectible<T>, to: address) {
        transfer::transfer(nft, to);
    }

    /// Implementable `transfer::transfer_to_object` function.
    public fun transfer_to_object<T: store + drop, S: key + store>(_w: T, nft: Collectible<T>, obj: &mut S) {
        transfer::transfer_to_object(nft, obj)
    }

    /// Implementable `transfer::transfer_to_object_id` function.
    public fun transfer_to_object_id<T: store + drop>(_w: T, nft: Collectible<T>, owner_id: &mut UID) {
        transfer::transfer_to_object_id(nft, owner_id)
    }

    /// Implementable `transfer::share_object` function.
    public fun share_object<T: store + drop>(_w: T, nft: Collectible<T>) {
        transfer::share_object(nft);
    }
}
