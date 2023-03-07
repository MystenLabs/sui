// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

///  This module is an implementation of an English auction
///  (https://en.wikipedia.org/wiki/English_auction) using single-owner
///  objects only. There are three types of parties participating in an
///  auction:
///  - auctioneer - a trusted party that runs the auction
///  - owner - the original owner of an item that is sold at an
///    auction; the owner submits a request to an auctioneer which runs
///    the auction
///  - bidders - parties interested in purchasing items sold
///    at an auction; they submit bids to an auctioneer to affect the
///    state of an auction
///
///  A typical lifetime of an auction looks as follows:
///  - The auction starts with the owner sending an item to be sold along with
///    its own address to the auctioneer who creates and initializes an
///    auction.
///  - Bidders send their bid to the auctioneer.
///    A bid consists of the funds offered for the item and the bidder's address.
///  - The auctioneer periodically inspects the bids:
///    - (inspected bid > current best bid (initially there is no bid)):
///      The auctioneer updates the auction with the current bid
///      and the funds of the previous highest bid are sent back to their owner.
///    - (inspected bid <= current best bid):
///      The auctioneer sents the inspected bid's funds back to the new bidder,
///      and the auction remains unchanged.
///  - The auctioneer eventually ends the auction:
///    - if no bids were received, the item goes back to the original owner
///    - otherwise the funds accumulated in the auction go to the
///      original owner and the item goes to the bidder that won the auction

module nfts::auction {
    use sui::coin::{Self, Coin};
    use sui::balance::Balance;
    use sui::sui::SUI;
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self,TxContext};

    use nfts::auction_lib::{Self, Auction};

    // Error codes.

    /// A bid submitted for the wrong (e.g. non-existent) auction.
    const EWrongAuction: u64 = 1;

    /// Represents a bid sent by a bidder to the auctioneer.
    struct Bid has key {
        id: UID,
        /// Address of the bidder
        bidder: address,
        /// ID of the Auction object this bid is intended for
        auction_id: ID,
        /// Coin used for bidding.
        bid: Balance<SUI>
    }

    // Entry functions.

    /// Creates an auction. It would be more natural to generate
    /// auction_id in create_auction and be able to return it so that
    /// it can be shared with bidders but we cannot do this at the
    /// moment. This is executed by the owner of the asset to be
    /// auctioned.
    public fun create_auction<T: key + store>(
        to_sell: T, auctioneer: address, ctx: &mut TxContext
    ): ID {
        let auction = auction_lib::create_auction(to_sell, ctx);
        let id = object::id(&auction);
        auction_lib::transfer(auction, auctioneer);
        id
    }

    /// Creates a bid a and send it to the auctioneer along with the
    /// ID of the auction. This is executed by a bidder.
    public fun bid(
        coin: Coin<SUI>, auction_id: ID, auctioneer: address, ctx: &mut TxContext
    ) {
        let bid = Bid {
            id: object::new(ctx),
            bidder: tx_context::sender(ctx),
            auction_id,
            bid: coin::into_balance(coin),
        };

        transfer::transfer(bid, auctioneer);
    }

    /// Updates the auction based on the information in the bid
    /// (update auction if higher bid received and send coin back for
    /// bids that are too low). This is executed by the auctioneer.
    public entry fun update_auction<T: key + store>(
        auction: &mut Auction<T>, bid: Bid, ctx: &mut TxContext
    ) {
        let Bid { id, bidder, auction_id, bid: balance } = bid;
        assert!(object::borrow_id(auction) == &auction_id, EWrongAuction);
        auction_lib::update_auction(auction, bidder, balance, ctx);

        object::delete(id);
    }

    /// Ends the auction - transfers item to the currently highest
    /// bidder or to the original owner if no bids have been
    /// placed. This is executed by the auctioneer.
    public entry fun end_auction<T: key + store>(
        auction: Auction<T>, ctx: &mut TxContext
    ) {
        auction_lib::end_and_destroy_auction(auction, ctx);
    }
}
