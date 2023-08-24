// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implements Collection Bidding. Currently it's a Marketplace-only functionality.
///
/// It is important that the bidder chooses the Marketplace, not the buyer.
module kiosk::collection_bidding_ext {
    use std::option::Option;
    use std::type_name;
    use std::vector;

    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
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
    use sui::pay;
    use sui::bag;

    use kiosk::personal_kiosk;
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
    /// Trying to accept a bid that does not exist.
    const EBidNotFound: u64 = 5;
    /// Trying to place a bid with no coins.
    const ENoCoinsPassed: u64 = 6;

    /// A key for Extension storage - a single bid on an item of type `T` on a `Market`.
    struct Bid<phantom T, phantom Market> has copy, store, drop {}

    // === Events ===

    /// An event that is emitted when a new bid is placed.
    struct NewBid<phantom T, phantom Market> has copy, drop {
        kiosk_id: ID,
        bids: vector<u64>,
        kiosk_owner: Option<address>,
    }

    /// An event that is emitted when a bid is accepted.
    struct BidAccepted<phantom T, phantom Market> has copy, drop {
        kiosk_id: ID,
        item_id: ID,
        source_kiosk_owner: Option<address>,
        destination_kiosk_owner: Option<address>,
    }

    /// An event that is emitted when a bid is canceled.
    struct BidCanceled<phantom T, phantom Market> has copy, drop {
        kiosk_id: ID,
        kiosk_owner: Option<address>,
    }

    // === Extension ===

    /// Extension permissions - `place` and `lock`.
    const PERMISSIONS: u128 = 3;

    /// The Extension witness.
    struct Extension has drop {}

    /// Install the extension into the Kiosk.
    public fun add(self: &mut Kiosk, cap: &KioskOwnerCap, ctx: &mut TxContext) {
        ext::add(Extension {}, self, cap, PERMISSIONS, ctx)
    }

    // === Bidding logic ===

    /// Place a bid on any item in a collection (`T`). We do not assert that all
    /// the values in the `place_bids` are identical, the amounts are emitted
    /// in the event, the order is reversed.
    ///
    /// Use `sui::pay::split_n` to prepare the Coins for the bid.
    public fun place_bids<T: key + store, Market>(
        self: &mut Kiosk, cap: &KioskOwnerCap, bids: vector<Coin<SUI>>, _ctx: &mut TxContext
    ) {
        assert!(vector::length(&bids) > 0, ENoCoinsPassed);
        assert!(kiosk::has_access(self, cap), ENotAuthorized);

        let amounts = vector[];
        let (i, count) = (0, vector::length(&bids));
        while (i < count) {
            vector::push_back(&mut amounts, coin::value(vector::borrow(&bids, i)));
            i = i + 1;
        };

        event::emit(NewBid<T, Market> {
            kiosk_id: object::id(self),
            bids: amounts,
            kiosk_owner: personal_kiosk::try_owner(self)
        });

        bag::add(ext::storage_mut(Extension {}, self), Bid<T, Market> {}, bids);
    }

    /// Cancel all bids, return the funds to the owner.
    public fun cancel_all<T: key + store, Market>(
        self: &mut Kiosk, cap: &KioskOwnerCap, ctx: &mut TxContext
    ): Coin<SUI> {
        assert!(kiosk::has_access(self, cap), ENotAuthorized);

        event::emit(BidCanceled<T, Market> {
            kiosk_id: object::id(self),
            kiosk_owner: personal_kiosk::try_owner(self)
        });

        let coins = bag::remove(ext::storage_mut(Extension {}, self), Bid<T, Market> {});
        let total = coin::zero(ctx);
        pay::join_vec(&mut total, coins);
        total
    }

    /// Accept the bid and make a purchase on in the `Kiosk`.
    ///
    /// 1. The seller creates a `MarketPurchaseCap` using the Marketplace adapter,
    /// and passes the Cap to this function.
    ///
    /// 2. The `bid` is taken from the extension storage and is used to purchase
    /// the item with the `MarketPurchaseCap`. Proceeds go to the seller's Kiosk.
    ///
    /// 3. The item is placed in the seller's Kiosk using the `place` or `lock`
    /// functions (see `PERMISSIONS`).
    public fun accept_market_bid<T: key + store, Market>(
        destination: &mut Kiosk,
        source: &mut Kiosk,
        mkt_cap: MarketPurchaseCap<T, Market>,
        policy: &TransferPolicy<T>,
        // keeping these arguments for extendability
        _lock: bool,
        ctx: &mut TxContext
    ): (TransferRequest<T>, TransferRequest<Market>) {
        let storage = ext::storage_mut(Extension {}, destination);
        assert!(bag::contains(storage, Bid<T, Market> {}), EBidNotFound);

        // Take 1 Coin from the bag - this is our bid (bids can't be empty, we make sure of it).
        let bid = vector::pop_back(bag::borrow_mut(storage, Bid<T, Market> {}));

        // If there are no bids left, remove the bag and the key from the storage.
        if (bid_count<T, Market>(destination) == 0) {
            vector::destroy_empty<Coin<SUI>>(
                bag::remove(
                    ext::storage_mut(Extension {}, destination),
                    Bid<T, Market> {}
                )
            );
        };

        assert!(ext::is_enabled<Extension>(destination), EExtensionDisabled);
        assert!(mkt::kiosk(&mkt_cap) == object::id(source), EIncorrectKiosk);
        assert!(mkt::min_price(&mkt_cap) <= coin::value(&bid), EIncorrectAmount);
        assert!(type_name::get<Market>() != type_name::get<NoMarket>(), EIncorrectMarketArg);

        // Perform the purchase operation in the seller's Kiosk using the `Bid`.
        let (item, request, market_request) = mkt::purchase(source, mkt_cap, bid, ctx);

        event::emit(BidAccepted<T, Market> {
            kiosk_id: object::id(destination),
            item_id: object::id(&item),
            destination_kiosk_owner: personal_kiosk::try_owner(destination),
            source_kiosk_owner: personal_kiosk::try_owner(source)
        });

        // Place of lock the item in the Buyer's Kiosk.
        place_or_lock(destination, item, policy);

        (request, market_request)
    }

    // === Getters ===

    /// Number of currently active bids.
    public fun offers_count(self: &Kiosk): u64 {
        bag::length(ext::storage(Extension {}, self))
    }

    /// Number of bids on an item of type `T` on a `Market` in a `Kiosk`.
    public fun bid_count<T: key + store, Market>(self: &Kiosk): u64 {
        let coins = bag::borrow(ext::storage(Extension {}, self), Bid<T, Market> {});
        vector::length<Coin<SUI>>(coins)
    }

    /// Returns the amount of the bid on an item of type `T` on a `Market`.
    /// The `NoMarket` generic can be used to check an item listed off the market.
    public fun bid_amount<T: key + store, Market>(self: &Kiosk): u64 {
        let coins = bag::borrow(ext::storage(Extension {}, self), Bid<T, Market> {});
        coin::value(vector::borrow<Coin<SUI>>(coins, 0))
    }

    // === Internal ===

    /// A helper function which either places or locks an item in the Kiosk depending
    /// on the Rules set in the `TransferPolicy`.
    fun place_or_lock<T: key + store>(kiosk: &mut Kiosk, item: T, policy: &TransferPolicy<T>) {
        let should_lock = vec_set::contains(policy::rules(policy), &type_name::get<LockRule>());
        if (should_lock) {
            ext::lock(Extension {}, kiosk, item, policy)
        } else {
            ext::place(Extension {}, kiosk, item, policy)
        };
    }
}
