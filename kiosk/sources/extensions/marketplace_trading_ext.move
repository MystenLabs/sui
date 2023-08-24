// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This extension implements the default list-purchase flow but for a specific
/// market (using the Marketplace Adapter).
///
/// Consists of 3 functions:
/// - list
/// - delist
/// - purchase
module kiosk::marketplace_trading_ext {
    use std::option::Option;

    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::transfer_policy::TransferRequest;
    use sui::kiosk_extension as ext;
    use sui::tx_context::TxContext;
    use sui::object::{Self, ID};
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::event;
    use sui::bag;

    use kiosk::personal_kiosk;
    use kiosk::marketplace_adapter::{Self as mkt, MarketPurchaseCap};

    /// For when the caller is not the owner of the Kiosk.
    const ENotOwner: u64 = 0;
    /// Trying to purchase or delist an item that is not listed.
    const ENotListed: u64 = 1;
    /// The payment is not enough to purchase the item.
    const EInsufficientPayment: u64 = 2;

    // === Events ===

    /// An item has been listed on a Marketplace.
    struct ItemListed<phantom T, phantom Market> has copy, drop {
        kiosk_id: ID,
        item_id: ID,
        price: u64,
        kiosk_owner: Option<address>
    }

    /// An item has been delisted from a Marketplace.
    struct ItemDelisted<phantom T, phantom Market> has copy, drop {
        kiosk_id: ID,
        item_id: ID,
        kiosk_owner: Option<address>
    }

    /// An item has been purchased from a Marketplace.
    struct ItemPurchased<phantom T, phantom Market> has copy, drop {
        kiosk_id: ID,
        item_id: ID,
        kiosk_owner: Option<address>
    }

    // === Extension ===

    /// The Extension Witness
    struct Extension has drop {}

    /// This Extension does not require any permissions.
    const PERMISSIONS: u128 = 0;

    /// Adds the Extension
    public fun add(kiosk: &mut Kiosk, cap: &KioskOwnerCap, ctx: &mut TxContext) {
        ext::add(Extension {}, kiosk, cap, PERMISSIONS, ctx)
    }

    // === Trading Functions ===

    /// List an item on a specified Marketplace.
    public fun list<T: key + store, Market>(
        self: &mut Kiosk,
        cap: &KioskOwnerCap,
        item_id: ID,
        price: u64,
        ctx: &mut TxContext
    ) {
        assert!(kiosk::has_access(self, cap), ENotOwner);

        let mkt_cap = mkt::new<T, Market>(self, cap, item_id, price, ctx);
        bag::add(ext::storage_mut(Extension {}, self), item_id, mkt_cap);

        event::emit(ItemListed<T, Market> {
            kiosk_owner: personal_kiosk::try_owner(self),
            kiosk_id: object::id(self),
            item_id,
            price,
        });
    }

    /// Delist an item from a specified Marketplace.
    public fun delist<T: key + store, Market>(
        self: &mut Kiosk,
        cap: &KioskOwnerCap,
        item_id: ID,
        ctx: &mut TxContext
    ) {
        assert!(kiosk::has_access(self, cap), ENotOwner);
        assert!(is_listed<T, Market>(self, item_id), ENotListed);

        let mkt_cap = bag::remove(ext::storage_mut(Extension {}, self), item_id);
        mkt::return_cap<T, Market>(self, mkt_cap, ctx);

        event::emit(ItemDelisted<T, Market> {
            kiosk_owner: personal_kiosk::try_owner(self),
            kiosk_id: object::id(self),
            item_id
        });
    }

    /// Purchase an item from a specified Marketplace.
    public fun purchase<T: key + store, Market>(
        self: &mut Kiosk,
        item_id: ID,
        payment: Coin<SUI>,
        ctx: &mut TxContext
    ): (T, TransferRequest<T>, TransferRequest<Market>) {
        assert!(is_listed<T, Market>(self, item_id), ENotListed);

        let mkt_cap = bag::remove(ext::storage_mut(Extension {}, self), item_id);
        assert!(coin::value(&payment) >= mkt::min_price(&mkt_cap), EInsufficientPayment);

        event::emit(ItemPurchased<T, Market> {
            kiosk_owner: personal_kiosk::try_owner(self),
            kiosk_id: object::id(self),
            item_id
        });

        mkt::purchase(self, mkt_cap, payment, ctx)
    }

    // === Getters ===

    /// Check if an item is currently listed on a specified Marketplace.
    public fun is_listed<T: key + store, Market>(self: &Kiosk, item_id: ID): bool {
        bag::contains_with_type<ID, MarketPurchaseCap<T, Market>>(
            ext::storage(Extension {}, self),
            item_id
        )
    }

    /// Get the price of a currently listed item from a specified Marketplace.
    public fun price<T: key + store, Market>(self: &Kiosk, item_id: ID): u64 {
        let mkt_cap = bag::borrow(ext::storage(Extension {}, self), item_id);
        mkt::min_price<T, Market>(mkt_cap)
    }
}
