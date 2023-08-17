// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implements Collection Bidding.
///
/// It is important that the bidder chooses the Marketplace, not the buyer.
module kiosk::collection_bidding_ext {
    use std::type_name;

    use sui::kiosk::{Self, Kiosk, KioskOwnerCap, PurchaseCap};
    use sui::kiosk_extension as ext;
    use sui::tx_context::TxContext;
    use sui::coin::{Self, Coin};
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferRequest,
    };
    use sui::sui::SUI;
    use sui::vec_set;
    use sui::object::{Self, ID};
    use sui::event;
    use sui::bag;

    use kiosk::kiosk_lock_rule::Rule as LockRule;
    use kiosk::marketplace_adapter::{Self as mkt, MarketPurchaseCap, NoMarket};

    /// Trying to perform an action in another user's Kiosk.
    const ENotAuthorized: u64 = 0;
    /// Trying to accept the bid in a disabled extension.
    const EExtensionDisabled: u64 = 1;
    /// A `PurchaseCap` was created in a different Kiosk.
    const EIncorrectKiosk: u64 = 2;
    /// The bid amount is less than the minimum price.
    const EIncorrectAmount: u64 = 3;
    /// Trying to accept a bid using a wrong function.
    const EIncorrectMarketArg: u64 = 4;

    /// Extension permissions - `place` and `lock`.
    const PERMISSIONS: u128 = 3;

    /// The Extension witness.
    struct Extension has drop {}

    /// A key for Extension storage - a single bid on an item of type `T` on a `Market`.
    struct Bid<phantom T, phantom Market> has copy, store, drop {}

    // === Events ===

    /// An event that is emitted when a new bid is placed.
    struct NewBid<phantom T, phantom Market> has copy, drop {
        kiosk_id: ID,
        bid: u64,
    }

    /// An event that is emitted when a bid is accepted.
    struct BidAccepted<phantom T, phantom Market> has copy, drop {
        kiosk_id: ID,
        item_id: ID,
    }

    /// An event that is emitted when a bid is canceled.
    struct BidCanceled<phantom T, phantom Market> has copy, drop {
        kiosk_id: ID,
    }

    // === Extension ===

    /// Install the extension into the Kiosk.
    public fun add(self: &mut Kiosk, cap: &KioskOwnerCap, ctx: &mut TxContext) {
        ext::add(Extension {}, self, cap, PERMISSIONS, ctx)
    }

    // === Bidding logic ===

    /// Place a bid on any item in a collection (`T`).
    public fun bid<T: key + store, Market>(
        self: &mut Kiosk, cap: &KioskOwnerCap, bid: Coin<SUI>, _ctx: &mut TxContext
    ) {
        event::emit(NewBid<T, Market> {
            kiosk_id: object::id(self),
            bid: coin::value(&bid),
        });

        assert!(kiosk::has_access(self, cap), ENotAuthorized);
        bag::add(ext::storage_mut(Extension {}, self), Bid<T, Market> {}, bid);
    }

    /// Cancel a bid, return the funds to the owner.
    public fun cancel<T: key + store, Market>(
        self: &mut Kiosk, cap: &KioskOwnerCap, _ctx: &mut TxContext
    ): Coin<SUI> {
        event::emit(BidCanceled<T, Market> {
            kiosk_id: object::id(self),
        });

        assert!(kiosk::has_access(self, cap), ENotAuthorized);
        bag::remove(ext::storage_mut(Extension {}, self), Bid<T, Market> {})
    }

    /// Accept a bid on any item in a collection (`T`).
    ///
    /// To do so, the selling party needs to create a `PurchaseCap<T>` with the
    /// value of the bid (or lower) and call the `accept` function.
    ///
    /// Internally, the item will be purchased from the seller's Kiosk for the
    /// value of the bid, and then placed into the buyer's Kiosk using the ext
    /// permissions (`lock` or `place` - depending on the `TransferPolicy`).
    public fun accept<T: key + store>(
        destination: &mut Kiosk,
        source: &mut Kiosk,
        purchase_cap: PurchaseCap<T>,
        policy: &TransferPolicy<T>,

        // consider removing this parameter and always follow optimistic approach:
        // if the policy does not contain `kiosk_lock_rule` - do `place`. For now
        // keeping it in case we discover a case when we need to do `lock`, and
        // the function signature won't need to change.
        _lock: bool,
        _ctx: &mut TxContext
    ): TransferRequest<T> {
        let bid: Coin<SUI> = bag::remove(ext::storage_mut(Extension {}, destination), Bid<T, NoMarket> {});
        let should_lock = vec_set::contains(policy::rules(policy), &type_name::get<LockRule>());

        assert!(ext::is_enabled<Extension>(destination), EExtensionDisabled);
        assert!(kiosk::purchase_cap_kiosk(&purchase_cap) == object::id(source), EIncorrectKiosk);
        assert!(kiosk::purchase_cap_min_price(&purchase_cap) <= coin::value(&bid), EIncorrectAmount);

        let (item, request) = kiosk::purchase_with_cap(source, purchase_cap, bid);

        event::emit(BidAccepted<T, NoMarket> {
            kiosk_id: object::id(destination),
            item_id: object::id(&item),
        });

        if (should_lock) {
            ext::lock(Extension {}, destination, item, policy)
        } else {
            ext::place(Extension {}, destination, item, policy);
        };

        request
    }

    /// Follows the same flow as the `accept` function but also returns the TransferRequest<Market>.
    public fun accept_market<T: key + store, Market>(
        destination: &mut Kiosk,
        source: &mut Kiosk,
        purchase_cap: MarketPurchaseCap<T, Market>,
        policy: &TransferPolicy<T>,

        _lock: bool,
        _ctx: &mut TxContext
    ): (TransferRequest<T>, TransferRequest<Market>) {
        let bid: Coin<SUI> = bag::remove(ext::storage_mut(Extension {}, destination), Bid<T, Market> {});
        let should_lock = vec_set::contains(policy::rules(policy), &type_name::get<LockRule>());

        assert!(ext::is_enabled<Extension>(destination), EExtensionDisabled);
        assert!(mkt::kiosk(&purchase_cap) == object::id(source), EIncorrectKiosk);
        assert!(mkt::min_price(&purchase_cap) <= coin::value(&bid), EIncorrectAmount);
        assert!(type_name::get<Market>() != type_name::get<NoMarket>(), EIncorrectMarketArg);

        let (item, request, market_request) = mkt::purchase(source, purchase_cap, bid);

        event::emit(BidAccepted<T, Market> {
            kiosk_id: object::id(destination),
            item_id: object::id(&item),
        });

        if (should_lock) {
            ext::lock(Extension {}, destination, item, policy)
        } else {
            ext::place(Extension {}, destination, item, policy);
        };

        (request, market_request)
    }
}
