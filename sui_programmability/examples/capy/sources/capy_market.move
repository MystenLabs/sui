// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// CapyMarket for Capy-related objects.
/// Allows selling  and accessories.
///
/// TODO: refactor usage of events - many of the parameters are redundant
/// and can be linked off-chain with additional tooling. Kept for usability
/// and development speed purposes.
module capy::capy_market {
    use sui::object::{Self, UID, ID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::pay;
    use sui::sui::SUI;
    use sui::event::emit;
    use sui::coin::{Self, Coin};
    use sui::dynamic_object_field as dof;

    use std::vector as vec;

    // The Capy Manager gains all control over admin actions
    // of the capy_marketplace. Modules must be published together
    // to achieve consistency over types.
    use capy::capy::{Capy, CapyManagerCap};

    /// For when someone tries to delist without ownership.
    const ENotOwner: u64 = 0;

    /// For when amount paid does not match the expected.
    const EAmountIncorrect: u64 = 1;

    /// For when there's nothing to claim from the marketplace.
    const ENoProfits: u64 = 2;

    // ======= Types =======

    /// A generic marketplace for anything.
    struct CapyMarket<phantom T: key> has key {
        id: UID,
    }

    /// A listing for the marketplace. Intermediary object which owns an Item.
    struct Listing has key, store {
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
        listing_id: ID,
        item_id: ID,
        price: u64,
        owner: address,
    }

    /// Emitted when owner delists an item from the CapyMarket<T>.
    struct ItemDelisted<phantom T> has copy, drop {
        listing_id: ID,
        item_id: ID,
    }

    /// Emitted when someone makes a purchase. `new_owner` shows
    /// who's a happy new owner of the purchased item.
    struct ItemPurchased<phantom T> has copy, drop {
        listing_id: ID,
        item_id: ID,
        new_owner: address,
    }

    /// For when someone collects profits from the market. Helps
    /// indexer show who has how much.
    struct ProfitsCollected<phantom T> has copy, drop {
        owner: address,
        amount: u64
    }

    // ======= Publishing =======

    #[allow(unused_function)]
    /// By default create two Markets
    fun init(ctx: &mut TxContext) {
        publish<Capy>(ctx);
    }

    /// Admin-only method which allows creating a new marketplace.
    public entry fun create_marketplace<T: key + store>(
        _: &CapyManagerCap, ctx: &mut TxContext
    ) {
        publish<T>(ctx)
    }

    /// Publish a new CapyMarket for any type T. Method is private and
    /// can only be called in a module initializer or in an admin-only
    /// method `create_marketplace`
    fun publish<T: key + store>(ctx: &mut TxContext) {
        let id = object::new(ctx);
        emit(MarketCreated<T> { market_id: object::uid_to_inner(&id) });
        transfer::share_object(CapyMarket<T> { id });
    }

    // ======= CapyMarket Actions =======

    /// List a batch of T at once.
    public fun batch_list<T: key + store>(
        market: &mut CapyMarket<T>,
        items: vector<T>,
        price: u64,
        ctx: &mut TxContext
    ) {
        while (vec::length(&items) > 0) {
            list(market, vec::pop_back(&mut items), price, ctx)
        };

        vec::destroy_empty(items);
    }

    /// List a new item on the CapyMarket.
    public entry fun list<T: key + store>(
        market: &mut CapyMarket<T>,
        item: T,
        price: u64,
        ctx: &mut TxContext
    ) {
        let id = object::new(ctx);
        let owner = tx_context::sender(ctx);
        let listing = Listing { id, price, owner };

        emit(ItemListed<T> {
            item_id: object::id(&item),
            listing_id: object::id(&listing),
            price,
            owner
        });

        // Attach Item to the Listing through listing.id;
        // Then attach listing to the marketplace through item_id;
        dof::add(&mut listing.id, true, item);
        dof::add(&mut market.id, object::id(&listing), listing);
    }

    /// Remove listing and get an item back. Only owner can do that.
    public fun delist<T: key + store>(
        market: &mut CapyMarket<T>,
        listing_id: ID,
        ctx: &TxContext
    ): T {
        let Listing { id, price: _, owner } = dof::remove<ID, Listing>(&mut market.id, listing_id);
        let item = dof::remove(&mut id, true);

        assert!(tx_context::sender(ctx) == owner, ENotOwner);

        emit(ItemDelisted<T> {
            listing_id,
            item_id: object::id(&item),
        });

        object::delete(id);
        item
    }

    /// Call [`delist`] and transfer item to the sender.
    entry fun delist_and_take<T: key + store>(
        market: &mut CapyMarket<T>,
        listing_id: ID,
        ctx: &TxContext
    ) {
        transfer::public_transfer(
            delist(market, listing_id, ctx),
            tx_context::sender(ctx)
        )
    }

    /// Withdraw profits from the marketplace as a single Coin (accumulated as a DOF).
    /// Uses sender of transaction to determine storage and control access.
    entry fun take_profits<T: key + store>(
        market: &mut CapyMarket<T>,
        ctx: &TxContext
    ) {
        let sender = tx_context::sender(ctx);
        assert!(dof::exists_(&market.id, sender), ENoProfits);
        let profit = dof::remove<address, Coin<SUI>>(&mut market.id, sender);

        emit(ProfitsCollected<T> {
            owner: sender,
            amount: coin::value(&profit)
        });

        transfer::public_transfer(profit, sender)
    }

    /// Purchase an item using a known Listing. Payment is done in Coin<C>.
    /// Amount paid must match the requested amount. If conditions are met,
    /// owner of the item gets the payment and buyer receives their item.
    public fun purchase<T: key + store>(
        market: &mut CapyMarket<T>,
        listing_id: ID,
        paid: Coin<SUI>,
        ctx: &TxContext
    ): T {
        let Listing { id, price, owner } = dof::remove<ID, Listing>(&mut market.id, listing_id);
        let item = dof::remove(&mut id, true);
        let new_owner = tx_context::sender(ctx);

        assert!(price == coin::value(&paid), EAmountIncorrect);

        emit(ItemPurchased<T> {
            item_id: object::id(&item),
            listing_id,
            new_owner
        });

        // if there's a balance attached to the marketplace - merge it with paid.
        // if not -> leave a Coin hanging as a dynamic field of the marketplace.
        if (dof::exists_(&market.id, owner)) {
            coin::join(dof::borrow_mut<address, Coin<SUI>>(&mut market.id, owner), paid)
        } else {
            dof::add(&mut market.id, owner, paid)
        };

        object::delete(id);
        item
    }

    /// Call [`buy`] and transfer item to the sender.
    entry fun purchase_and_take<T: key + store>(
        market: &mut CapyMarket<T>,
        listing_id: ID,
        paid: Coin<SUI>,
        ctx: &TxContext
    ) {
        transfer::public_transfer(
            purchase(market, listing_id, paid, ctx),
            tx_context::sender(ctx)
        )
    }

    /// Use `&mut Coin<SUI>` to purchase `T` from marketplace.
    entry fun purchase_and_take_mut<T: key + store>(
        market: &mut CapyMarket<T>,
        listing_id: ID,
        paid: &mut Coin<SUI>,
        ctx: &mut TxContext
    ) {
        let listing = dof::borrow<ID, Listing>(&market.id, *&listing_id);
        let coin = coin::split(paid, listing.price, ctx);
        purchase_and_take(market, listing_id, coin, ctx)
    }

    /// Send multiple Coins in order to merge them and afford pricy Capy.
    entry fun purchase_and_take_mul_coins<T: key + store>(
        market: &mut CapyMarket<T>,
        listing_id: ID,
        coins: vector<Coin<SUI>>,
        ctx: &mut TxContext
    ) {
        let listing = dof::borrow<ID, Listing>(&market.id, *&listing_id);
        let coin = vec::pop_back(&mut coins);

        pay::join_vec(&mut coin, coins);

        let paid = coin::split(&mut coin, listing.price, ctx);
        transfer::public_transfer(coin, tx_context::sender(ctx));
        purchase_and_take(market, listing_id, paid, ctx)
    }
}
