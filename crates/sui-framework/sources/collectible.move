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
        /// Incremental number in a collection.
        unique_number: u64,
        /// Total number of items in collection. Improves
        /// on-chain discovery (eg when Cap is wrapped).
        edition: Option<u64>,
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


    /// Borrow `unique_number` field.
    public fun unique_number<T: store + drop>(self: &Collectible<T>): u64 { self.unique_number }

    /// Borrow `data` field.
    public fun info<T: store + drop>(self: &Collectible<T>): &T { &self.info }

    /// Implementable method giving a mutable reference to `data`.
    public fun info_mut<T: store + drop>(_w: T, self: &mut Collectible<T>): &mut T { &mut self.info }

    /// Get a mutable reference to UID for allowing `transfer_to_object_id` with `Collectible` type.
    ///
    /// `Collectible` does not have `store` ability and it means that `transfer_to_object` call can
    /// never be used on the `Collectible`. But there's a solution to allow it having children -
    /// by using `transfer_to_object_id` which requires a mutable reference to UID. For theses purposes
    /// UID needs to be conditionally exposed to outer world.
    ///
    /// By adding implementation for this call publicly or by using it inside the module with the
    /// implementation, it is possible to transfer objects TO `Collectible`s.
    public fun uid_mut<T: store + drop>(_w: T, self: &mut Collectible<T>): &mut UID { &mut self.id }


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
            edition: *&cap.max_supply,
            unique_number: cap.total_supply,
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
        transfer::share_object(nft)
    }
}

#[test_only]
/// Defines a `Default` Collectible standard which only has two fields: url and name;
///
/// Collectibles of this type are:
/// - limited (max_supply is required)
/// - freely transferable;
/// - non-modifyable;
/// - attacheable to other objects on the network;
module sui::default_collectible {
    use sui::collectible::{Self, Collectible, CollectionManagerCap};
    use sui::tx_context::{TxContext};
    use sui::url::{Self, Url};
    use sui::types;

    use std::string::{Self, String};
    use std::option;

    /// A standard for NFTs that have a `name` and a `url` fields.
    struct Default<phantom T> has store, drop {
        name: String,
        url: Url
    }

    /// Get the name of the Collectible.
    public fun name<T>(c: &Collectible<Default<T>>): &String {
        &collectible::info(c).name
    }

    /// Get the url of the Collectible.
    public fun url<T>(c: &Collectible<Default<T>>): &Url {
        &collectible::info(c).url
    }

    /// Create new collection with the `Default` metadata. To call this function
    /// a one-time-witness needs to be supplied. Resulting `CollectionManagerCap<T>`
    /// is guraranteed to be unique in the system.
    public fun create_collection<T: drop>(
        w: T,
        max_supply: u64,
        ctx: &mut TxContext
    ): CollectionManagerCap<Default<T>> {
        types::is_one_time_witness(&w);

        collectible::create_collection(null(), option::some(max_supply), ctx)
    }

    /// Mint a new `Collectible<Default<T>>`.
    public fun mint<T>(
        c: &mut CollectionManagerCap<Default<T>>,
        name: vector<u8>,
        url: vector<u8>,
        ctx: &mut TxContext
    ): Collectible<Default<T>> {
        let data = Default {
            name: string::utf8(name),
            url: url::new_unsafe_from_bytes(url)
        };

        collectible::mint(c, data, ctx)
    }

    /// Mint a new `Collectible` and transfer it to the specified address.
    public entry fun mint_and_transfer<T>(
        c: &mut CollectionManagerCap<Default<T>>,
        name: vector<u8>,
        url: vector<u8>,
        to: address,
        ctx: &mut TxContext
    ) {
        transfer(mint(c, name, url, ctx), to);
    }

    /// Free for all transfer function for any `Collectible<Default<T>>`.
    public entry fun transfer<T>(c: Collectible<Default<T>>, to: address) {
        collectible::transfer(null(), c, to)
    }

    /// Transfer to object implementation for `Collectible<Default<T>>`
    public entry fun transfer_to_object<T, S: key + store>(c: Collectible<Default<T>>, obj: &mut S) {
        collectible::transfer_to_object(null(), c, obj)
    }

    /// Private function for creating an empty witness struct.
    /// `Default<T>` works both as a Collectible metadata and as a witness.
    fun null<T>(): Default<T> {
        Default {
            name: string::utf8(b""),
            url: url::new_unsafe_from_bytes(b"")
        }
    }
}
