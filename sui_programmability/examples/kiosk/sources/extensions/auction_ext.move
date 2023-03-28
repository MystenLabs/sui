// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implements an English auction extension for the Kiosk.
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

        let old_coin = option::swap(&mut auction_mut.current_bid, payment);
        let old_bidder = auction_mut.current_leader;

        assert!(epoch(ctx) <= auction_mut.until_epoch, ETooLate);
        assert!(paid_amt > auction_mut.start_price, EBidTooLow);
        assert!(coin::value(&old_coin) > paid_amt, EBidTooLow);

        sui::transfer::public_transfer(old_coin, old_bidder);
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

    /// An auction can only be canceled at any time if there hasn't been
    /// a single bid yet. The item is returned to the `Kiosk` and the `PurchaseCap`
    /// is destroyed.
    public fun cancel<T: key + store>(
        kiosk: &mut Kiosk, item_id: ID
    ) {
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
