// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implements an English auction extension for the Kiosk. The auction
/// logic is straightforward: the auction is started by calling `start`,
/// an `AuctionStarted` event is emitted and the item is avalable for
/// bidding. The auction ends when the `until_epoch` epoch is reached.
///
/// - If there is no bid, the auction can be `cancel`-ed by the kiosk owner.
/// - If there is, the auction can be `end`-ed by the highest bidder. In this
/// case the item and the `TransferRequest` are returned to the caller to
/// apply transfer policy and give an option to move the item.
module kiosk::auction_ext {
    use std::option::{Self, Option};
    use sui::dynamic_field as df;
    use sui::transfer_policy::TransferRequest;
    use sui::kiosk::{Self, Kiosk, PurchaseCap, KioskOwnerCap};
    use sui::tx_context::{sender, epoch, TxContext};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, ID};
    use sui::sui::SUI;
    use sui::event;

    /// Bid is lower than the previous one.
    const EBidTooLow: u64 = 0;
    /// Auction is over.
    const ETooLate: u64 = 1;
    /// Auction is not over yet.
    const ETooSoon: u64 = 2;
    /// No bid has been made; cannot end the auction.
    const ENoBid: u64 = 3;
    /// Only the current leader can end the auction.
    const ENotWinner: u64 = 4;
    /// Only the owner can cancel the auction.
    const ENotOwner: u64 = 5;

    /// Stores Auction configuration as well as the state (current bid,
    /// current winner, etc.)
    struct Auction<phantom T: key + store> has store {
        purchase_cap: PurchaseCap<T>,
        current_bid: Option<Coin<SUI>>,
        current_leader: address,
        start_price: u64,
        until_epoch: u64
    }

    /// Dynamic field key for the `PurchaseCap` of the auctioned item.
    /// To support multiple auctions in parallel uses `item_id` as
    /// a uniqueness guarantee.
    struct AuctionedKey has copy, drop, store { item_id: ID }

    /// Emitted when an Auction is started
    struct AuctionStarted<phantom T> has copy, drop, store {
        kiosk_id: ID,
        item_id: ID,
        start_price: u64,
        until_epoch: u64
    }

    /// Start the auction by issuing a `PurchaseCap` and locking the
    /// asset in the `Kiosk`. Fails if `KioskOwnerCap` does not match
    /// the `Kiosk` or if `item_id` is not found in the `Kiosk`.
    public fun start<T: key + store>(
        kiosk: &mut Kiosk,
        kiosk_cap: &KioskOwnerCap,
        item_id: ID,
        start_price: u64,
        until_epoch: u64,
        ctx: &mut TxContext
    ) {
        let purchase_cap = kiosk::list_with_purchase_cap<T>(
            kiosk, kiosk_cap, item_id, start_price, ctx
        );

        event::emit(AuctionStarted<T> {
            kiosk_id: object::id(kiosk),
            item_id,
            start_price,
            until_epoch
        });

        df::add(
            kiosk::uid_mut(kiosk),
            AuctionedKey { item_id },
            Auction {
                purchase_cap,
                current_bid: option::none(),
                current_leader: sender(ctx),
                start_price,
                until_epoch
            }
        )
    }

    /// Make a bid. To succeed it must be higher than the previous.
    public fun bid<T: key + store>(
        kiosk: &mut Kiosk, item_id: ID, payment: Coin<SUI>, ctx: &TxContext
    ) {
        let paid_amt = coin::value(&payment);
        let auction_mut: &mut Auction<T> = df::borrow_mut(
            kiosk::uid_mut(kiosk),
            AuctionedKey { item_id }
        );

        if (option::is_none(&auction_mut.current_bid)) {
            assert!(paid_amt >= auction_mut.start_price, EBidTooLow);
            option::fill(&mut auction_mut.current_bid, payment)
        } else {
            let old_coin = option::swap(&mut auction_mut.current_bid, payment);
            let old_bidder = auction_mut.current_leader;

            assert!(epoch(ctx) <= auction_mut.until_epoch, ETooLate);
            assert!(coin::value(&old_coin) < paid_amt, EBidTooLow);

            sui::transfer::public_transfer(old_coin, old_bidder)
        };


        auction_mut.current_leader = sender(ctx)
    }

    /// End the auction and release the asset (along with `TransferRequest`).
    /// Fails if the auction is not over yet, if the sender is not the current
    /// leader or if there is no bid.
    public fun end<T: key + store>(
        kiosk: &mut Kiosk, item_id: ID, ctx: &mut TxContext
    ): (T, TransferRequest<T>) {
        let Auction<T> {
            purchase_cap,
            current_bid,
            current_leader,
            start_price: _,
            until_epoch
        } = df::remove(
            kiosk::uid_mut(kiosk),
            AuctionedKey { item_id }
        );

        assert!(sender(ctx) == current_leader, ENotWinner);
        assert!(option::is_some(&current_bid), ENoBid);
        assert!(epoch(ctx) > until_epoch, ETooSoon);

        kiosk::purchase_with_cap(
            kiosk,
            purchase_cap,
            option::destroy_some(current_bid)
        )
    }

    /// An auction can only be canceled by the kiosk owner at any time if there
    /// hasn't been a single bid yet. The item is returned to the `Kiosk` and
    /// the `PurchaseCap` is destroyed.
    public fun cancel<T: key + store>(
        kiosk: &mut Kiosk, kiosk_cap: &KioskOwnerCap, item_id: ID
    ) {
        assert!(kiosk::has_access(kiosk, kiosk_cap), ENotOwner);
        let Auction<T> {
            purchase_cap,
            current_bid,
            start_price: _,
            current_leader: _,
            until_epoch: _
        } = df::remove(
            kiosk::uid_mut(kiosk),
            AuctionedKey { item_id }
        );

        assert!(option::is_none(&current_bid), ENoBid);
        // assert!(epoch(ctx) > until_epoch, ETooSoon);

        option::destroy_none(current_bid);
        kiosk::return_purchase_cap(kiosk, purchase_cap)
    }
}

#[test_only]
module kiosk::auction_ext_tests {
    use kiosk::auction_ext;
    use sui::kiosk::{Self, Kiosk};
    use sui::transfer_policy as policy;
    use sui::object::ID;
    use sui::kiosk_test_utils::{
        Self as utils,
        Asset
    };
    use sui::transfer::{public_share_object, public_transfer};
    use sui::test_scenario::{
        Self as test,
        Scenario,
        ctx,
    };
    use sui::tx_context::{
        sender,
        increment_epoch_number as next_epoch
    };

    #[test]
    fun test_auction_flow() {
        let (alice, bob, carl) = utils::folks();

        // Alice: creates a Kiosk and places an asset;
        // Epoch: 0
        let test = test::begin(alice);
        let item_id = prepare(&mut test);

        // Bob: learns that an auction has started and makes a bid;
        // Epoch: 0 -> 1
        let effects = test::next_tx(&mut test, bob); {
            let kiosk = test::take_shared<Kiosk>(&test);
            let ctx = ctx(&mut test);

            auction_ext::bid<Asset>(&mut kiosk, item_id, utils::get_sui(100, ctx), ctx);
            test::return_shared(kiosk);
            next_epoch(ctx);
        };

        // make sure an event is emitted
        assert!(test::num_user_events(&effects) == 1, 0);

        // Carl: makes a higher bid;
        // Epoch: 1 -> 4
        test::next_tx(&mut test, carl); {
            let kiosk = test::take_shared<Kiosk>(&test);
            let ctx = ctx(&mut test);

            auction_ext::bid<Asset>(&mut kiosk, item_id, utils::get_sui(200, ctx), ctx);
            test::return_shared(kiosk);
            next_epoch(ctx);
            next_epoch(ctx);
            next_epoch(ctx);
        };

        // Carl: ends the auction and receives the asset;
        // Epoch: 3
        test::next_tx(&mut test, carl); {
            let kiosk = test::take_shared<Kiosk>(&test);
            let ctx = ctx(&mut test);
            let (item, transfer_req) = auction_ext::end<Asset>(&mut kiosk, item_id, ctx);

            assert!(policy::item(&transfer_req) == item_id, 0);
            assert!(policy::paid(&transfer_req) == 200, 1);
            assert!(policy::from(&transfer_req) == sui::object::id(&kiosk), 2);

            test::return_shared(kiosk);

            // Hack: we need to confirm transfer request
            let (policy, policy_cap) = utils::get_policy(ctx);
            policy::confirm_request(&policy, transfer_req);
            utils::return_policy(policy, policy_cap, ctx);
            public_transfer(item, carl);
        };

        test::end(test);
    }

    /// Prepare the test scenario:
    /// - sender creates a Kiosk and places an asset
    /// - item_id is returned for further use
    /// - KioskOwnerCap is sent to sender
    /// - Auction for item is started
    ///
    /// Params:
    /// - start_price = 0
    /// - until_epoch = 3
    fun prepare(test: &mut Scenario): ID {
        let (item, item_id) = utils::get_asset(ctx(test));
        let (kiosk, kiosk_cap) = utils::get_kiosk(ctx(test));

        kiosk::place(&mut kiosk, &kiosk_cap, item);
        auction_ext::start<Asset>(&mut kiosk, &kiosk_cap, item_id, 0, 3, ctx(test));

        public_transfer(kiosk_cap, sender(ctx(test)));
        public_share_object(kiosk);
        (item_id)
    }
}
