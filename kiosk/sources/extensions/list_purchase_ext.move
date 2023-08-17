// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This extension implements the default list-purchase flow but for a specific
/// market (using the Marketplace Adapter).
///
/// Consists of 3 functions:
/// - list
/// - delist
/// - purchase
module kiosk::fixed_price_ext {
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::transfer_policy::TransferRequest;
    use sui::kiosk_extension as ext;
    use sui::tx_context::TxContext;
    use sui::object::{Self, ID};
    use sui::coin::Coin;
    use sui::sui::SUI;
    use sui::event;
    use sui::bag;

    use kiosk::marketplace_adapter as mkt;

    /// For when the caller is not the owner of the Kiosk.
    const ENotOwner: u64 = 0;

    /// The Extension Witness
    struct Extension has drop {}

    /// This Extension does not require any permissions.
    const PERMISSIONS: u128 = 0;

    struct ItemListed<phantom T, phantom Market> has copy, drop {
        kiosk_id: ID,
        item_id: ID,
        price: u64
    }

    struct ItemDelisted<phantom T, phantom Market> has copy, drop {
        kiosk_id: ID,
        item_id: ID
    }

    struct ItemPurchased<phantom T, phantom Market> has copy, drop {
        kiosk_id: ID,
        item_id: ID,
    }

    /// Adds the Extension
    public fun add(kiosk: &mut Kiosk, cap: &KioskOwnerCap, ctx: &mut TxContext) {
        ext::add(Extension {}, kiosk, cap, PERMISSIONS, ctx)
    }

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
            kiosk_id: object::id(self),
            item_id,
            price
        });
    }

    /// Delist an item from a specified Marketplace.
    public fun delist<T: key + store, Market>(
        self: &mut Kiosk,
        cap: &KioskOwnerCap,
        item_id: ID,
        _ctx: &mut TxContext
    ) {
        assert!(kiosk::has_access(self, cap), ENotOwner);

        let mkt_cap = bag::remove(ext::storage_mut(Extension {}, self), item_id);
        mkt::return_cap<T, Market>(self, mkt_cap);

        event::emit(ItemDelisted<T, Market> {
            kiosk_id: object::id(self),
            item_id
        });
    }

    public fun purchase<T: key + store, Market>(
        self: &mut Kiosk,
        item_id: ID,
        payment: Coin<SUI>,
        _ctx: &mut TxContext
    ): (T, TransferRequest<T>, TransferRequest<Market>) {
        let mkt_cap = bag::remove(ext::storage_mut(Extension {}, self), item_id);

        event::emit(ItemPurchased<T, Market> {
            kiosk_id: object::id(self),
            item_id
        });

        mkt::purchase(self, mkt_cap, payment)
    }
}
