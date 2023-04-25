// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The module which defines the `Collectible` type. It is an all-in-one
/// package to create a `Display`, a `Publisher` and a `TransferPolicy` to
/// enable `Kiosk` trading from the start.
module kiosk::collectible {
    use sui::transfer;
    use std::vector as vec;
    use std::string::String;
    use std::option::{Self, Option};
    use sui::package::{Self, Publisher};
    use sui::display::{Self, Display};
    use sui::borrow::{Self, Referent, Borrow};
    use sui::tx_context::{sender, TxContext};
    use sui::object::{Self, UID};
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicyCap
    };

    /// Trying to `claim_ticket` with a non OTW struct.
    const ENotOneTimeWitness: u64 = 0;
    /// The type parameter `T` is not from the same module as the `OTW`.
    const ETypeNotFromModule: u64 = 1;
    /// Maximum size of the Collection is reached - minting forbidden.
    const ECapReached: u64 = 2;
    /// Names length does not match `image_urls` length
    const EWrongNamesLength: u64 = 3;
    /// Descriptions length does not match `image_urls` length
    const EWrongDescriptionsLength: u64 = 4;
    /// Creators length does not match `image_urls` length
    const EWrongCreatorsLength: u64 = 5;
    /// Metadatas length does not match `image_urls` length
    const EWrongMetadatasLength: u64 = 6;

    /// Centralized registry to provide access to system features of
    /// the Collectible.
    struct Registry has key {
        id: UID,
        publisher: Publisher
    }

    /// One-in-all capability wrapping all necessary functions such as
    /// `Display`, `PolicyCap` and the `Publisher`.
    struct CollectionCap<T: store> has key, store {
        id: UID,
        publisher: Referent<Publisher>,
        display: Referent<Display<Collectible<T>>>,
        policy_cap: Referent<TransferPolicyCap<Collectible<T>>>,
        max_supply: Option<u32>,
        minted: u32,
        burned: u32,
    }

    /// Special object which connects init function and the collection
    /// initialization.
    struct CollectionTicket<phantom T: store> has key, store {
        id: UID,
        publisher: Publisher,
        max_supply: Option<u32>
    }

    /// Basic collectible containing most of the fields from the proposed
    /// Display set. The `metadata` field is a generic type which can be
    /// used to store any custom data.
    struct Collectible<T: store> has key, store {
        id: UID,
        image_url: String,
        name: Option<String>,
        description: Option<String>,
        creator: Option<String>,
        meta: Option<T>,
    }

    /// OTW to initialize the Registry and the base type.
    struct COLLECTIBLE has drop {}

    /// Create the centralized Registry of Collectibles to provide access
    /// to the Publisher functionality of the Collectible.
    fun init(otw: COLLECTIBLE, ctx: &mut TxContext) {
        transfer::share_object(Registry {
            id: object::new(ctx),
            publisher: package::claim(otw, ctx)
        })
    }

    /// Called in the external module initializer. Sends a `CollectionTicket`
    /// to the transaction sender which then enables them to initialize the
    /// Collection.
    ///
    /// - The OTW parameter is a One-Time-Witness;
    /// - The T parameter is the expected Metadata / custom type to use for
    /// the Collection;
    public fun claim_ticket<OTW: drop, T: store>(otw: OTW, max_supply: Option<u32>, ctx: &mut TxContext) {
        assert!(sui::types::is_one_time_witness(&otw), ENotOneTimeWitness);

        let publisher = package::claim(otw, ctx);

        assert!(package::from_module<T>(&publisher), ETypeNotFromModule);
        transfer::transfer(CollectionTicket<T> {
            id: object::new(ctx),
            publisher,
            max_supply
        }, sender(ctx));
    }

    /// Use the `CollectionTicket` to start a new collection and receive a
    /// `CollectionCap`.
    public fun create_collection<T: store>(
        registry: &Registry,
        ticket: CollectionTicket<T>,
        ctx: &mut TxContext
    ): CollectionCap<T> {
        let CollectionTicket { id, publisher, max_supply } = ticket;
        object::delete(id);

        let display = display::new<Collectible<T>>(&registry.publisher, ctx);
        let (policy, policy_cap) = policy::new<Collectible<T>>(
            &registry.publisher, ctx
        );

        transfer::public_share_object(policy);

        CollectionCap<T> {
            id: object::new(ctx),
            display: borrow::new(display, ctx),
            publisher: borrow::new(publisher, ctx),
            policy_cap: borrow::new(policy_cap, ctx),
            max_supply,
            minted: 0,
            burned: 0,
        }
    }

    // === Minting ===

    /// Mint a single Collectible specifying the fields.
    /// Can only be performed by the owner of the `CollectionCap`.
    public fun mint<T: store>(
        cap: &mut CollectionCap<T>,
        image_url: String,
        name: Option<String>,
        description: Option<String>,
        creator: Option<String>,
        meta: Option<T>,
        ctx: &mut TxContext
    ): Collectible<T> {
        assert!(option::is_none(&cap.max_supply) || *option::borrow(&cap.max_supply) > cap.minted, ECapReached);
        cap.minted = cap.minted + 1;

        Collectible {
            id: object::new(ctx),
            image_url,
            name,
            description,
            creator,
            meta
        }
    }

    /// Batch mint a vector of Collectibles specifying the fields. Lengths of
    /// the optional fields must match the length of the `image_urls` vector.
    /// Metadata vector is also optional, which
    public fun batch_mint<T: store>(
        cap: &mut CollectionCap<T>,
        image_urls: vector<String>,
        names: Option<vector<String>>,
        descriptions: Option<vector<String>>,
        creators: Option<vector<String>>,
        metas: Option<vector<T>>,
        ctx: &mut TxContext
    ) {
    // ): vector<Collectible<T>> {
        let len = vec::length(&image_urls);
        // let res = vec::empty();

        // perform a dummy check to make sure collection does not overflow
        // safe to downcast since the length will never be greater than u32::MAX
        assert!(
            option::is_none(&cap.max_supply)
            || cap.minted + (len as u32) < *option::borrow(&cap.max_supply)
        , ECapReached);

        assert!(
            option::is_none(&names)
            || vec::length(option::borrow(&names)) == len
        , EWrongNamesLength);

        assert!(
            option::is_none(&creators)
            || vec::length(option::borrow(&creators)) == len
        , EWrongCreatorsLength);

        assert!(
            option::is_none(&descriptions)
            || vec::length(option::borrow(&descriptions)) == len
        , EWrongDescriptionsLength);

        assert!(
            option::is_none(&metas)
            || vec::length(option::borrow(&metas)) == len
        , EWrongMetadatasLength);

        while (len > 0) {
            // vec::push_back(&mut res, mint(
            let obj = mint(
                cap,
                vec::pop_back(&mut image_urls),
                pop_or_none(&mut names),
                pop_or_none(&mut descriptions),
                pop_or_none(&mut creators),
                pop_or_none(&mut metas),
                ctx
            );

            sui::transfer::transfer(obj, sender(ctx));
            // ));

            len = len - 1;
        };

        if (option::is_some(&metas)) {
            let metas = option::destroy_some(metas);
            vec::destroy_empty(metas)
        } else {
            option::destroy_none(metas);
        };

        // res
    }

    // === Borrowing methods ===

    /// Take the `TransferPolicyCap` from the `CollectionCap`.
    public fun borrow_policy_cap<T: store>(
        self: &mut CollectionCap<T>
    ): (TransferPolicyCap<Collectible<T>>, Borrow) {
        borrow::borrow(&mut self.policy_cap)
    }

    /// Return the `TransferPolicyCap` to the `CollectionCap`. Must be called if
    /// the capability was borrowed, or a transaction would fail.
    public fun return_policy_cap<T: store>(
        self: &mut CollectionCap<T>,
        cap: TransferPolicyCap<Collectible<T>>,
        borrow: Borrow
    ) {
        borrow::put_back(&mut self.policy_cap, cap, borrow)
    }

    /// Take the `Display` from the `CollectionCap`.
    public fun borrow_display<T: store>(
        self: &mut CollectionCap<T>
    ): (Display<Collectible<T>>, Borrow) {
        borrow::borrow(&mut self.display)
    }

    /// Return the `Display` to the `CollectionCap`. Must be called if
    /// the capability was borrowed, or a transaction would fail.
    public fun return_display<T: store>(
        self: &mut CollectionCap<T>,
        display: Display<Collectible<T>>,
        borrow: Borrow
    ) {
        borrow::put_back(&mut self.display, display, borrow)
    }

    /// Take the `Publisher` from the `CollectionCap`.
    public fun borrow_publisher<T: store>(
        self: &mut CollectionCap<T>
    ): (Publisher, Borrow) {
        borrow::borrow(&mut self.publisher)
    }

    /// Return the `Publisher` to the `CollectionCap`. Must be called if
    /// the capability was borrowed, or a transaction would fail.
    public fun return_publisher<T: store>(
        self: &mut CollectionCap<T>,
        publisher: Publisher,
        borrow: Borrow
    ) {
        borrow::put_back(&mut self.publisher, publisher, borrow)
    }

    // === Internal ===

    /// Pop the value from the optional vector or return `none`.
    fun pop_or_none<T>(opt: &mut Option<vector<T>>): Option<T> {
        if (option::is_none(opt)) {
            option::none()
        } else {
            option::some(vec::pop_back(option::borrow_mut(opt)))
        }
    }
}
