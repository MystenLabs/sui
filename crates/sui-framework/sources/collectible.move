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
    use sui::object::{Self, ID, UID};
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

    /// Get ID for pointing to the `Collectible`.
    public fun id<T: store + drop>(self: &Collectible<T>): ID { object::uid_to_inner(&self.id) }

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
/// Defines a `Tricky` Collectible standard which has few important properties:
///
/// - Can be traded on multiple marketplaces at the same time;
/// - Considered owned while listed on a marketplace;
/// - This standard provides ownership management while keeping metadata definition
/// up to the implementers;
///
///
/// The way it is implemented is the following:
/// - Every Collectible<Tricky> is a shared object with the `owner` field which
/// implements logical ownership.
/// - As long as user is the owner he is free to change ownership to any other address
/// - At any point owner can issue `TransferCap`s - objects granting permission to
/// perform logical transfer of the `Collectible`
/// - `TransferCap`s can be listed on marketplaces and have flexible set of abilities:
/// `key` and `store` which make them compatible with most of the applications
///
/// Edge cases:
/// - Each `TransferCap` is locked by number of transfers performed over the Collectible;
/// meaning that if transfer has been performed, `TransferCap` can only be burned
/// - Once the first `TransferCap` is issued, the object is locked and no longer
/// considered "owned" by its owner
/// - Owner can't use `TransferCap` to transfer to his address; to gain full access over
/// the Collectible he has to burn all `TransferCap`s therefore unlocking the object
/// - As soon as `TransferCap` used, all other `TransferCap`s are useless, and the owner
/// should have been changed
///
/// TODO: extend this standard to support any metadata inside.
module sui::tricky_collectible {
    use sui::collectible::{Self, Collectible, CollectionManagerCap};
    use sui::tx_context::{Self, TxContext};
    use sui::object::{Self, UID, ID};
    use std::option::{Option};

    /// For when someone is trying to transfer a `Collectible` for which
    /// at least one `TransferCap` is issued.
    const ECollectibleLocked: u64 = 0;

    /// For when someone tries to perform a transfer over someone else's object
    const ENotOwner: u64 = 1;

    /// For when multiple transfer caps were issued and one of them was already used.
    const ETransferAlreadyPerformed: u64 = 2;

    /// For when owner tries to use a `TransferCap`.
    const EOwner: u64 = 3;

    /// For when ID of the `TransferCap` does not match `Collectible.id`
    const EIdMismatch: u64 = 4;


    /// This is a standard for a `Collectible`. Instead of using owned transfer
    /// functions, `Tricky` is based on logical ownership and using a shared
    /// object instead.
    ///
    /// Unlike its owned buddies, Tricky has few advantages...
    struct Tricky<T: store + drop> has store, drop {
        /// Logical owner of the `Collectible`. Transfering
        /// it is as simple as changing this field.
        owner: address,
        /// Number of TransferCaps issued for this `Collectible`. For
        /// an object to be freely transferable, this number has to equal `0`.
        transfer_caps_issued: u8,
        /// Total number of transfers performed over this `Collectible`.
        /// Necessary to determine whether a `TransferCap` is still relevant.
        transfers: u64,
        /// Inner field that stores `Collectible<Tricky>` metadata.
        info: T
    }

    /// Issuable capability to perform a single transfer of the `Collectible`.
    /// Can be issued more than once, and is active while number of transfers
    /// matches the `collectible.info.transfers` field. After that loses its
    /// ability to perform a transfer.
    ///
    /// Should be used to list a `Collectible` on different marketplaces or
    /// other platforms. The first `TransferCap` used to perform a transfer
    /// locks all others.
    struct TransferCap<phantom T> has key, store {
        id: UID,
        /// ID of the `Collectible` for which `TransferCap` is issued.
        target: ID,
        /// Counter which has to match `Tricky.transfers`. Combination of
        /// both: `target` and `issue` fields is required to perform a
        /// transfer of the `Collectible<Tricky<T>>`.
        issue: u64
    }


    /// Read `owner` field from `Collectible<Tricky<T>>`.
    public fun owner<T: store + drop>(self: &Collectible<Tricky<T>>): address {
        collectible::info(self).owner
    }

    /// Whether this Collectible is locked for transfers. Once at least
    /// one `TransferCap` is issued, it remails locked until all of them
    /// are returned or at least one of them is used to transfer the object.
    public fun is_locked<T: store + drop>(self: &Collectible<Tricky<T>>): bool {
        collectible::info(self).transfer_caps_issued == 0
    }


    /// Create `CollectionManagerCap` the same way we would do it for
    /// any other `Collectible`s standard.
    /// No check for OTW here -> it is meant to be built on top.
    public fun create_collection<T: store + drop>(
        w: T,
        max_supply: Option<u64>,
        ctx: &mut TxContext
    ): CollectionManagerCap<Tricky<T>> {
        collectible::create_collection(null(w), max_supply, ctx)
    }

    /// Mint new `Collectible<Tricky<T>>` and give logical ownership to
    /// the `owner`; the `Collectible` itself is shared.
    public fun mint_for<T: store + drop>(
        w: T, // really hard to remove this buddy from here
        cap: &mut CollectionManagerCap<Tricky<T>>,
        info: T,
        owner: address,
        ctx: &mut TxContext
    ) {
        collectible::share_object(
            null<T>(w),
            collectible::mint(cap, Tricky<T> {
                info,
                owner,
                transfer_caps_issued: 0,
                transfers: 0,
            }, ctx)
        )
    }

    /// Transfer a `Collectible` by changing logical ownership. Can only
    /// be performed if no `TransferCap`s were issued for this object and
    /// if sender is an `owner` of the `Collectible`.
    public fun transfer<T: store + drop>(
        w: T,
        c: &mut Collectible<Tricky<T>>,
        to: address,
        ctx: &mut TxContext
    ) {
        assert!(is_locked(c) == false, ECollectibleLocked);
        assert!(owner(c) == tx_context::sender(ctx), ENotOwner);

        let info_mut = collectible::info_mut(null(w), c);

        info_mut.owner = to;
        info_mut.transfers = info_mut.transfers + 1;
    }

    /// Issue a `TransferCap` therefore locking an object. Even if object is
    /// locked, additional `TransferCap`s can be issued. Owner-only action.
    public fun issue_transfer_cap<T: store + drop>(
        w: T,
        c: &mut Collectible<Tricky<T>>,
        ctx: &mut TxContext
    ): TransferCap<T> {
        assert!(owner(c) == tx_context::sender(ctx), ENotOwner);

        let info_mut = collectible::info_mut(null(w), c);
        info_mut.transfer_caps_issued = info_mut.transfer_caps_issued + 1;

        TransferCap {
            id: object::new(ctx),
            target: collectible::id(c),
            issue: collectible::info(c).transfers
        }
    }

    /// Burn `TransferCap` to decrease the number of actively issued Caps.
    /// If the TransferCap is outdated, then don't update the issued number.
    public fun burn_transfer_cap<T: store + drop>(
        w: T,
        c: &mut Collectible<Tricky<T>>,
        cap: TransferCap<T>
    ) {
        assert!(collectible::id(c) == cap.target, EIdMismatch);

        let info_mut = collectible::info_mut(null(w), c);

        // If `TransferCap` matches the
        if (info_mut.transfers == cap.issue) {
            info_mut.transfer_caps_issued = info_mut.transfer_caps_issued - 1;
        };

        let TransferCap { id, target: _, issue: _ } = cap;
        object::delete(id);
    }

    /// Use a `TransferCap` to change ownership of the object. Since
    /// owner can actually use it to cheat on marketplaces by resetting
    /// number, we restrict changing the owner field to the same one.
    public fun use_transfer_cap<T: store + drop>(
        w: T,
        c: &mut Collectible<Tricky<T>>,
        cap: TransferCap<T>,
        owner: address,
    ) {
        assert!(owner(c) != owner, EOwner);
        assert!(collectible::id(c) == object::uid_to_inner(&cap.id), EIdMismatch);

        let info_mut = collectible::info_mut(null(w), c);

        assert!(info_mut.transfers == cap.issue, ETransferAlreadyPerformed);

        info_mut.transfers = info_mut.transfers + 1;
        info_mut.owner = owner;

        let TransferCap { id, target: _, issue: _ } = cap;
        object::delete(id);
    }

    /// `Null` object for witness implementations.
    fun null<T: store + drop>(w: T): Tricky<T> {
        Tricky {
            owner: @0x0,
            transfer_caps_issued: 0,
            transfers: 0,
            info: w
        }
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
    public entry fun transfer_to_object<T, S: key + store>(
        c: Collectible<Default<T>>,
        obj: &mut S
    ) {
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
