// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// CapyMarket - a generic Marketplace for capy-related assets.
/// Currently, allows selling Capys and accessories.
///
/// The structure of the Markeptlace storage is the following:
/// ```
///                  /+---(item_id)--> Listing<T> ---(bool)--> Item #1
/// ( CapyMarket<T> ) +---(item_id)--> Listing<T> ---(bool)--> Item #2
///                  \+---(item_id)--> Listing<T> ---(bool)--> Item #N
/// ```
module capy::capy_market {
    use sui::object::{Self, UID, ID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::sui::SUI;
    use sui::event::emit;
    use sui::coin::{Self, Coin};
    use sui::dynamic_object_field as dof;

    // The Capy Manager gains all control over admin actions
    // of the capy_marketplace. Modules must be published together
    // to achieve consistency over types.
    use capy::capy::{Capy, CapyManagerCap};
    use capy::capy_items::{CapyItem};

    // For when someone tries to delist without ownership.
    const ENotOwner: u64 = 1;

    // For when amount paid does not match the expected.
    const EAmountIncorrect: u64 = 0;

    // ======= Types =======

    /// A generic marketplace for anything.
    struct CapyMarket<phantom T: key> has key {
        id: UID,
    }

    /// A listing for the marketplace. Intermediary object which owns an Item.
    struct Listing<phantom T: key + store> has key, store {
        id: UID,
        price: u64,
        owner: address,
    }

    // ======= Events =======

    /// Emitted when a new CapyMarket is created.
    struct MarketCreated<phantom T> has copy, drop {
        market_id: ID,
    }

    /// Emitted when someone lists a new item on the CapyMarket<T>.
    struct ItemListed<phantom T> has copy, drop {
        item_id: ID,
        price: u64,
        owner: address,
    }

    /// Emitted when owner delists an item from the CapyMarket<T>.
    struct ItemDelisted<phantom T> has copy, drop {
        item_id: ID,
    }

    /// Emitted when someone makes a purchase. `new_owner` shows
    /// who is the new owner of the purchased asset.
    struct ItemPurchased<phantom T> has copy, drop {
        item_id: ID,
        new_owner: address,
    }

    // ======= Publishing =======

    /// By default create two Market
    // Turned off for managing
    fun init(ctx: &mut TxContext) {
        publish<Capy>(ctx);
        publish<CapyItem>(ctx);
    }

    /// Admin-only method which allows marketplace creation.
    public entry fun create_marketplace<T: key + store>(
        _: &CapyManagerCap, ctx: &mut TxContext
    ) {
        publish<T>(ctx)
    }

    /// Create and share a new `CapyMarket` for the type `T`. Method is private
    /// and can only be called in the module initializer or in the admin-only
    /// method `create_marketplace`.
    fun publish<T: key + store>(ctx: &mut TxContext) {
        let id = object::new(ctx);
        emit(MarketCreated<T> { market_id: object::uid_to_inner(&id) });
        transfer::share_object(CapyMarket<T> { id });
    }

    // ======= CapyMarket Actions =======

    /// List a new item on the `CapyMarket`.
    public entry fun list<T: key + store>(
        market: &mut CapyMarket<T>,
        item: T,
        price: u64,
        ctx: &mut TxContext
    ) {
        let id = object::new(ctx);
        let item_id = object::id(&item);
        let owner = tx_context::sender(ctx);

        emit(ItemListed<T> {
            item_id: *&item_id,
            price,
            owner
        });

        // First attach Item to the Listing with a boolean `true` value;
        // Then attach listing to the marketplace through `item.id`;
        dof::add(&mut id, true, item);
        dof::add(&mut market.id, item_id, Listing<T> { id, price, owner });
    }

    /// Remove listing and get an item back. Can only be performed by the `owner`.
    public fun delist<T: key + store>(
        market: &mut CapyMarket<T>,
        item_id: ID,
        ctx: &mut TxContext
    ): T {
        let Listing { id, price: _, owner } = dof::remove<ID, Listing<T>>(&mut market.id, item_id);
        let item = dof::remove(&mut id, true);

        assert!(tx_context::sender(ctx) == owner, ENotOwner);

        emit(ItemDelisted<T> {
            item_id: object::id(&item),
        });

        object::delete(id);
        item
    }

    /// Call [`delist`] and transfer item to the sender.
    public entry fun delist_and_take<T: key + store>(
        market: &mut CapyMarket<T>,
        item_id: ID,
        ctx: &mut TxContext
    ) {
        transfer::transfer(
            delist(market, item_id, ctx),
            tx_context::sender(ctx)
        )
    }

    /// Purchase an asset by the `item_id`. Payment is done in Coin<C>.
    /// Paid amount must match the requested amount. If conditions are met,
    /// the owner of the item gets the payment and the buyer receives their item.
    public fun purchase<T: key + store>(
        market: &mut CapyMarket<T>,
        item_id: ID,
        paid: Coin<SUI>,
        ctx: &mut TxContext
    ): T {
        let Listing { id, price, owner } = dof::remove<ID, Listing<T>>(&mut market.id, item_id);
        let item = dof::remove(&mut id, true);
        let new_owner = tx_context::sender(ctx);

        assert!(price == coin::value(&paid), EAmountIncorrect);

        emit(ItemPurchased<T> {
            item_id: object::id(&item),
            new_owner
        });

        transfer::transfer(paid, owner);
        object::delete(id);
        item
    }

    /// Call [`buy`] and transfer item to the sender.
    public entry fun purchase_and_take<T: key + store>(
        market: &mut CapyMarket<T>,
        item_id: ID,
        paid: Coin<SUI>,
        ctx: &mut TxContext
    ) {
        transfer::transfer(
            purchase(market, item_id, paid, ctx),
            tx_context::sender(ctx)
        )
    }

    /// Use `&mut Coin<SUI>` to purchase `T` from marketplace.
    public entry fun purchase_and_take_mut<T: key + store>(
        market: &mut CapyMarket<T>,
        item_id: ID,
        paid: &mut Coin<SUI>,
        ctx: &mut TxContext
    ) {
        let listing = dof::borrow<ID, Listing<T>>(&market.id, *&item_id);
        let coin = coin::split(paid, listing.price, ctx);
        purchase_and_take(market, item_id, coin, ctx)
    }
}
