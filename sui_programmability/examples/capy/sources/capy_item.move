// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Store for Capys. Unlike Marketplace, Store sells identical items
/// in the limit specified quantity or, if quantity is not set, unlimited.
///
/// Gives the Store Owner full access over the Listings and their quantity
/// as well as allows collecting profits in a single call.
module capy::capy_item {
    use sui::url::{Self, Url};
    use sui::object::{Self, ID, UID};
    use sui::tx_context::{sender, TxContext};
    use std::string::{Self, String};
    use sui::sui::SUI;
    use sui::balance::{Self, Balance};
    use std::option::{Self, Option};
    use sui::dynamic_object_field as dof;
    use sui::coin::{Self, Coin};
    use sui::transfer;
    use std::vector as vec;
    use sui::event::emit;
    use sui::pay;

    /// Base path for `CapyItem.url` attribute. Is temporary and improves
    /// explorer / wallet display. Always points to the dev/testnet server.
    const IMAGE_URL: vector<u8> = b"https://api.capy.art/items/";

    /// Store for any type T. Collects profits from all sold listings
    /// to be later acquirable by the Capy Admin.
    struct ItemStore has key {
        id: UID,
        balance: Balance<SUI>
    }

    /// A Capy item, that is being purchased from the `ItemStore`.
    struct CapyItem has key, store {
        id: UID,
        name: String,
        /// Urls and other meta information should
        /// always go last as it allows for partial
        /// deserialization of data on the frontend
        url: Url,
    }

    /// A Capability granting the bearer full control over the `ItemStore`.
    struct StoreOwnerCap has key, store { id: UID }

    /// A listing for an Item. Supply is either finite or infinite.
    struct ListedItem has key, store {
        id: UID,
        url: Url,
        name: String,
        type: String,
        price: u64,
        quantity: Option<u64>,
    }

    /// Emitted when new item is purchased.
    /// Off-chain we only need to know which ID
    /// corresponds to which name to serve the data.
    struct ItemCreated has copy, drop {
        id: ID,
        name: String,
    }

    #[allow(unused_function)]
    /// Create a `ItemStore` and a `StoreOwnerCap` for this store.
    fun init(ctx: &mut TxContext) {
        transfer::share_object(ItemStore {
            id: object::new(ctx),
            balance: balance::zero()
        });

        transfer::public_transfer(StoreOwnerCap {
            id: object::new(ctx)
        }, sender(ctx))
    }

    /// Admin action - collect Profits from the `ItemStore`.
    public entry fun collect_profits(
        _: &StoreOwnerCap, s: &mut ItemStore, ctx: &mut TxContext
    ) {
        let a = balance::value(&s.balance);
        let b = balance::split(&mut s.balance, a);

        transfer::public_transfer(coin::from_balance(b, ctx), sender(ctx))
    }

    /// Change the quantity value for the listing in the `ItemStore`.
    public entry fun set_quantity(
        _: &StoreOwnerCap, s: &mut ItemStore, name: vector<u8>, quantity: u64
    ) {
        let listing_mut = dof::borrow_mut<vector<u8>, ListedItem>(&mut s.id, name);
        option::swap(&mut listing_mut.quantity, quantity);
    }

    /// List an item in the `ItemStore` to be freely purchasable
    /// within the set quantity (if set).
    public entry fun sell(
        _: &StoreOwnerCap,
        s: &mut ItemStore,
        name: vector<u8>,
        type: vector<u8>,
        price: u64,
        // quantity: Option<u64>,
        ctx: &mut TxContext
    ) {
        dof::add(&mut s.id, name, ListedItem {
            id: object::new(ctx),
            url: img_url(name),
            price,
            quantity: option::none(), // temporarily only infinite quantity
            name: string::utf8(name),
            type: string::utf8(type)
        });
    }

    /// Buy an Item from the `ItemStore`. Pay `Coin<SUI>` and
    /// receive a `CapyItem`.
    public entry fun buy_and_take(
        s: &mut ItemStore, name: vector<u8>, payment: Coin<SUI>, ctx: &mut TxContext
    ) {
        let listing_mut = dof::borrow_mut<vector<u8>, ListedItem>(&mut s.id, name);

        // check that the Coin amount matches the price; then add it to the balance
        assert!(coin::value(&payment) == listing_mut.price, 0);
        coin::put(&mut s.balance, payment);

        // if quantity is set, make sure that it's not 0; then decrement
        if (option::is_some(&listing_mut.quantity)) {
            let q = option::borrow(&listing_mut.quantity);
            assert!(*q > 0, 0);
            option::swap(&mut listing_mut.quantity, *q - 1);
        };

        let id = object::new(ctx);

        emit(ItemCreated {
            id: object::uid_to_inner(&id),
            name: listing_mut.name
        });

        transfer::public_transfer(CapyItem {
            id,
            url: listing_mut.url,
            name: listing_mut.name,
        }, sender(ctx))
    }

    /// Buy a CapyItem with a single Coin which may be bigger than the
    /// price of the listing.
    public entry fun buy_mut(
        s: &mut ItemStore, name: vector<u8>, payment: &mut Coin<SUI>, ctx: &mut TxContext
    ) {
        let listing = dof::borrow<vector<u8>, ListedItem>(&mut s.id, name);
        let paid = coin::split(payment, listing.price, ctx);
        buy_and_take(s, name, paid, ctx)
    }

    /// Buy a CapyItem with multiple Coins by joining them first and then
    /// calling the `buy_mut` function.
    public entry fun buy_mul_coin(
        s: &mut ItemStore, name: vector<u8>, coins: vector<Coin<SUI>>, ctx: &mut TxContext
    ) {
        let paid = vec::pop_back(&mut coins);
        pay::join_vec(&mut paid, coins);
        buy_mut(s, name, &mut paid, ctx);
        transfer::public_transfer(paid, sender(ctx))
    }

    /// Construct an image URL for the `CapyItem`.
    fun img_url(name: vector<u8>): Url {
        let capy_url = *&IMAGE_URL;
        vec::append(&mut capy_url, name);
        vec::append(&mut capy_url, b"/svg");

        url::new_unsafe_from_bytes(capy_url)
    }
}
