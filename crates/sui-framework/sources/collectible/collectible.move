// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Small and simple implementation for the common collectible type.
/// Contains a basic set of fields, the only required one of which is `img_url`.
///
/// Other fields can be omitted by using an `option::none()`.
/// Custom metadata can be created and passed into the `Collectible` but that would
/// require additional work on the creator side to set up metadata creation methods.
module sui::collectible {
    use std::vector as vec;
    use std::string::String;
    use std::option::{Self, Option};
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;
    use sui::display::{Self, Display};
    use sui::package::{Self, Publisher};
    use sui::transfer_policy::{Self, TransferPolicy, TransferPolicyCap};
    use sui::borrow::{Self, Borrow, Referent};

    /// A witness type passed is not an OTW.
    const ENotOneTimeWitness: u64 = 0;
    /// The type `T` is not from the same module as the OTW.
    const EModuleDoesNotContainT: u64 = 1;
    /// Maximum size of the Collection is reached - minting forbidden.
    const ECapReached: u64 = 2;
    /// Names length does not match `img_urls` length
    const EWrongNamesLength: u64 = 3;
    /// Descriptions length does not match `img_urls` length
    const EWrongDescriptionsLength: u64 = 4;
    /// Creators length does not match `img_urls` length
    const EWrongCreatorsLength: u64 = 5;
    /// Metadatas length does not match `img_urls` length
    const EWrongMetadatasLength: u64 = 6;

    /// Basic collectible - should contain only unique information (eg
    /// if all collectibles have the same description, it should be put
    /// into the Display to apply to all of the objects of this type, and
    /// not in every object).
    struct Collectible<T: store> has key, store {
        id: UID,
        /// The only required parameter for the Collectible.
        /// Should only contain a unique part of the URL to be used in the
        /// template engine in the `Display` and save gas and storage costs.
        img_url: String,
        name: Option<String>,
        description: Option<String>,
        creator: Option<String>,
        meta: Option<T>,
    }

    /// Capability granted to the collection creator. Is guaranteed to be one
    /// per `T` in the `create_collection` function.
    /// Contains the cap - maximum amount of Collectibles minted.
    struct CollectionCreatorCap<T: store> has key, store {
        id: UID,
        max_supply: Option<u64>,
        display: Referent<Display<Collectible<T>>>,
        policy: Referent<TransferPolicyCap<Collectible<T>>>,
        minted: u64
    }

    /// Create a new collection and receive `CollectionCreatorCap` with a `Publisher`.
    ///
    /// To make sure that a collection is created only once, we require an OTW;
    /// but since the collection also requires a Publisher to set up the display,
    /// we create the Publisher object here as well.
    ///
    /// Type parameter `T` is phantom; so we constrain it via `Publisher` to be
    /// defined in the same module as the OTW. Aborts otherwise.
    public fun create_collection<OTW: drop, T: store>(
        otw: OTW, max_supply: Option<u64>, ctx: &mut TxContext
    ): (
        Publisher,
        TransferPolicy<Collectible<T>>,
        CollectionCreatorCap<T>,
    ) {
        assert!(sui::types::is_one_time_witness(&otw), ENotOneTimeWitness);

        let publisher = package::claim(otw, ctx);
        let display = display::new_protected<Collectible<T>>(ctx);
        let (policy, policy_cap) = transfer_policy::new_protected<Collectible<T>>(ctx);

        assert!(package::from_module<T>(&publisher), EModuleDoesNotContainT);

        (
            publisher,
            policy,
            CollectionCreatorCap<T> {
                id: object::new(ctx),
                minted: 0,
                max_supply,
                display: borrow::new(display, ctx),
                policy: borrow::new(policy_cap, ctx),
            }
        )
    }

    /// Mint a single Collectible specifying the fields.
    /// Can only be performed by the owner of the `CollectionCreatorCap`.
    public fun mint<T: store>(
        cap: &mut CollectionCreatorCap<T>,
        img_url: String,
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
            img_url,
            name,
            description,
            creator,
            meta
        }
    }

    /// Batch mint multiple Collectibles at once.
    /// Any of the fields can be omitted by supplying a `none()`.
    ///
    /// Field for custom metadata can be used for custom Collectibles.
    public fun batch_mint<T: store>(
        cap: &mut CollectionCreatorCap<T>,
        img_urls: vector<String>,
        names: Option<vector<String>>,
        descriptions: Option<vector<String>>,
        creators: Option<vector<String>>,
        metas: Option<vector<T>>,
        ctx: &mut TxContext
    ): vector<Collectible<T>> {
        let len = vec::length(&img_urls);
        let res = vec::empty();

        // perform a dummy check to make sure collection does not overflow
        assert!(option::is_none(&cap.max_supply) || cap.minted + len < *option::borrow(&cap.max_supply), ECapReached);
        assert!(option::is_none(&names) || vec::length(option::borrow(&names)) == len, EWrongNamesLength);
        assert!(option::is_none(&creators) || vec::length(option::borrow(&creators)) == len, EWrongCreatorsLength);
        assert!(option::is_none(&descriptions) || vec::length(option::borrow(&descriptions)) == len, EWrongDescriptionsLength);
        assert!(option::is_none(&metas) || vec::length(option::borrow(&metas)) == len, EWrongMetadatasLength);

        while (len > 0) {
            vec::push_back(&mut res, mint(
                cap,
                vec::pop_back(&mut img_urls),
                pop_or_none(&mut names),
                pop_or_none(&mut descriptions),
                pop_or_none(&mut creators),
                pop_or_none(&mut metas),
                ctx
            ));

            len = len - 1;
        };

        if (option::is_some(&metas)) {
            let metas = option::destroy_some(metas);
            vec::destroy_empty(metas)
        } else {
            option::destroy_none(metas);
        };

        res
    }

    // === Borrows ===

    /// Take the Display object.
    public fun borrow_display<T: store>(self: &mut CollectionCreatorCap<T>): (Display<Collectible<T>>, Borrow) {
        borrow::borrow(&mut self.display)
    }

    /// Return the Display object.
    public fun return_display<T: store>(
        self: &mut CollectionCreatorCap<T>, display: Display<Collectible<T>>, borrow: Borrow
    ) {
        borrow::put_back(&mut self.display, display, borrow)
    }

    // === Reads ===

    /// Keeping the door open for the dynamic field extensions.
    public fun uid_mut<T: store>(self: &mut Collectible<T>): &mut UID {
        &mut self.id
    }

    /// Read `img_url` field.
    public fun img_url<T: store>(self: &Collectible<T>): &String {
        &self.img_url
    }

    /// Read `name` field.
    public fun name<T: store>(self: &Collectible<T>): &Option<String> {
        &self.name
    }

    /// Read `description` field.
    public fun description<T: store>(self: &Collectible<T>): &Option<String> {
        &self.description
    }

    /// Read `creator` field.
    public fun creator<T: store>(self: &Collectible<T>): &Option<String> {
        &self.creator
    }

    /// Read `meta` field.
    public fun meta<T: store>(self: &Collectible<T>): &Option<T> {
        &self.meta
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
