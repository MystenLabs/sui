// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module kiosk::marketplace_ext {
    use sui::coin::{Self, Coin};
    use sui::dynamic_field as df;
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::object::ID;
    use sui::sui::SUI;
    use sui::transfer_policy::TransferRequest;
    use sui::tx_context::TxContext;

    /// Only owner can delist the item.
    const ENotOwner: u64 = 1;

    /// The key for the Marketplace listing.
    struct MarketplaceListingKey<phantom T> has store, copy, drop {
        item_id: ID
    }

    /// Lists an item in the Kiosk for the sale on a marketplace (requires witness).
    public fun list<Market: drop, T: key + store>(
        _market: Market,
        kiosk: &mut Kiosk,
        cap: &KioskOwnerCap,
        item_id: ID,
        price: u64,
        ctx: &mut TxContext
    ) {
        let purchase_cap = kiosk::list_with_purchase_cap<T>(kiosk, cap, item_id, price, ctx);
        df::add(
            kiosk::uid_mut(kiosk),
            MarketplaceListingKey<T>{ item_id },
            purchase_cap
        );
    }

    /// Purchase an item after receiving the Marketplace confirmation (the fee was paid).
    public fun purchase<Market: drop, T: key + store>(
        _market: Market,
        kiosk: &mut Kiosk,
        item_id: ID,
        payment: &mut Coin<SUI>,
        ctx: &mut TxContext
    ): (T, TransferRequest<T>) {
        let purchase_cap = df::remove(
            kiosk::uid_mut(kiosk),
            MarketplaceListingKey<T>{ item_id }
        );

        let to_pay = coin::split(payment, kiosk::purchase_cap_min_price(&purchase_cap), ctx);
        kiosk::purchase_with_cap(kiosk, purchase_cap, to_pay)
    }

    /// Delist an item and make it impossible to purchase.
    public fun delist<Market: drop, T: key + store>(
        kiosk: &mut Kiosk,
        cap: &KioskOwnerCap,
        item_id: ID,
    ) {
        assert!(kiosk::has_access(kiosk, cap), ENotOwner);

        let purchase_cap = df::remove(
            kiosk::uid_mut(kiosk),
            MarketplaceListingKey<T>{ item_id }
        );

        kiosk::return_purchase_cap<T>(kiosk, purchase_cap);
    }
}
