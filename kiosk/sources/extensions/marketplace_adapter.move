// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The best practical approach to trading on marketplaces and favoring their
/// fees and conditions is issuing an additional `TransferRequest` which requires
/// resolution in the marketplace. However, issuing another `TransferRequest`
/// is not always possible because it must be copied from TransferRequest<Item>,
/// mostly because the price of the sale is not known to the very moment of the
/// sale. And if there's already a TransferRequest<Item>, how do we enforce the
/// creation of an extra request?
///
/// To address this problem and also solve the extension interoperability issue,
/// we created a `marketplace_adapter` - simple utility which wraps the
/// `PurchaseCap` and handles the last step of the purchase flow in the Kiosk.
///
/// Unlike `PurchaseCap` purpose of which was to be "free", `MarketPurchaseCap`
/// - the wrapper - only comes with a `store` to reduce the amount of scenarios
/// when it is transferred by accident or sent to an address / object.
module kiosk::marketplace_adapter {
    use sui::transfer_policy::{Self as policy, TransferRequest};
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap, PurchaseCap};
    use sui::tx_context::TxContext;
    use sui::object::ID;
    use sui::coin::Coin;
    use sui::sui::SUI;

    /// The `NoMarket` type is used to provide a default `Market` type parameter
    /// for a scenario when the `MarketplaceAdapter` is not used and extensions
    /// maintain uniformity of emitted events. NoMarket = no marketplace.
    struct NoMarket {}

    /// The `MarketPurchaseCap` wraps the `PurchaseCap` and forces the unlocking
    /// party to satisfy the `TransferPolicy<Market>` requirements.
    struct MarketPurchaseCap<phantom T: key + store, phantom Market> has store {
        purchase_cap: PurchaseCap<T>
    }

    /// The `MarketKioskOwnerCap` wraps the `KioskOwnerCap` and forces the
    /// unlocking with a TransferRequest.
    public fun new<T: key + store, Market>(
        kiosk: &mut Kiosk,
        cap: &KioskOwnerCap,
        item_id: ID,
        min_price: u64,
        ctx: &mut TxContext
    ): MarketPurchaseCap<T, Market> {
        MarketPurchaseCap<T, Market> {
            purchase_cap: kiosk::list_with_purchase_cap(
                kiosk, cap, item_id, min_price, ctx
            )
        }
    }

    /// The `MarketKioskOwnerCap` wraps the `KioskOwnerCap` and forces the
    /// unlocking with a TransferRequest.
    public fun return_cap<T: key  + store, Market>(
        kiosk: &mut Kiosk,
        cap: MarketPurchaseCap<T, Market>
    ) {
        let MarketPurchaseCap { purchase_cap } = cap;
        kiosk::return_purchase_cap(kiosk, purchase_cap);
    }

    /// Use the `MarketPurchaseCap` to purchase an item from the `Kiosk`. Unlike
    /// the default flow, this function adds a `TransferRequest<Market>` which
    /// forces the unlocking party to satisfy the `TransferPolicy<Market>`
    public fun purchase<T: key + store, Market>(
        kiosk: &mut Kiosk,
        cap: MarketPurchaseCap<T, Market>,
        coin: Coin<SUI>
    ): (T, TransferRequest<T>, TransferRequest<Market>) {
        let MarketPurchaseCap { purchase_cap } = cap;
        let (item, request) = kiosk::purchase_with_cap(kiosk, purchase_cap, coin);
        let market_request = policy::new_request(
            policy::item(&request),
            policy::paid(&request),
            policy::from(&request),
        );

        (item, request, market_request)
    }

    /// Handy wrapper to read the `kiosk` field of the inner `PurchaseCap`
    public fun kiosk<T: key + store, Market>(self: &MarketPurchaseCap<T, Market>): ID {
        kiosk::purchase_cap_kiosk(&self.purchase_cap)
    }

    /// Handy wrapper to read the `item` field of the inner `PurchaseCap`
    public fun item<T: key + store, Market>(self: &MarketPurchaseCap<T, Market>): ID {
        kiosk::purchase_cap_item(&self.purchase_cap)
    }

    /// Handy wrapper to read the `min_price` field of the inner `PurchaseCap`
    public fun min_price<T: key + store, Market>(self: &MarketPurchaseCap<T, Market>): u64 {
        kiosk::purchase_cap_min_price(&self.purchase_cap)
    }
}
