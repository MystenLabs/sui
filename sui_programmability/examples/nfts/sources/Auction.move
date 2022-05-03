// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This is an implementation of an English auction
/// (https://en.wikipedia.org/wiki/English_auction) using single-owner
/// objects only. There are 3 types of parties participating in an
/// auction:
/// - auctioneer - this is a trusted party that runs the auction
/// - owner - this is the original owner of an item that is sold at an
/// auction; the owner submits a request to an auctioneer that runs
/// the auction
/// - bidders - these are parties interested in purchasing items sold
/// at an auction; they submit bids to an auctioneer to affect the
/// state of an auction
///
/// A typical lifetime of an auction looks as follows:
/// - auction starts by the owner sending an item to be sold along with
/// its own address to the auctioneer who creates and initializes an
/// auction
/// - bidders send bid to the auctioneer for a given auction
/// consisting of the funds they intend to use for the item's purchase
/// and their addresses
/// - the auctioneer periodically inspects the bids:
///   - if the inspected bid is higher than the current bid (initially
///   there is no bid), the auction is updated with the current bid
///   and funds representing previous highest bid are sent to the
///   original owner
///   - otherwise (bid is too low) the bidder's funds are sent back to
///   the bidder and the auction remains unchanged
/// - the auctioneer eventually ends the auction
///   - if no bids were received, the item goes back to the original owner
///   - otherwise the funds accumulated in the auction go to the
///   original owner and the item goes to the bidder that won the
///   auction
module NFTs::Auction {
    use Sui::Coin::{Self, Coin};
    use Sui::Balance::Balance;
    use Sui::SUI::SUI;
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self,TxContext};

    use NFTs::AuctionLib::{Self, Auction};

    // Error codes.

    /// A bid submitted for the wrong (e.g. non-existent) auction.
    const EWrongAuction: u64 = 1;

    /// Represents a bid sent by a bidder to the auctioneer.
    struct Bid has key {
        id: VersionedID,
        /// Address of the bidder
        bidder: address,
        /// ID of the Auction object this bid is intended for
        auction_id: ID,
        /// Coin used for bidding.
        bid: Balance<SUI>
    }

    // Entry functions.

    /// Creates an auction. It would be more natural to generate
    /// auction_id in crate_auction and be able to return it so that
    /// it can be shared with bidders but we cannot do this at the
    /// moment. This is executed by the owner of the asset to be
    /// auctioned.
    public fun create_auction<T: key + store>(
        to_sell: T, id: VersionedID, auctioneer: address, ctx: &mut TxContext
    ) {
        let auction = AuctionLib::create_auction(id, to_sell, ctx);
        Transfer::transfer(auction, auctioneer);
    }

    /// Creates a bid a and send it to the auctioneer along with the
    /// ID of the auction. This is executed by a bidder.
    public fun bid(
        coin: Coin<SUI>, auction_id: ID, auctioneer: address, ctx: &mut TxContext
    ) {
        let bid = Bid {
            id: TxContext::new_id(ctx),
            bidder: TxContext::sender(ctx),
            auction_id,
            bid: Coin::into_balance(coin),
        };

        Transfer::transfer(bid, auctioneer);
    }

    /// Updates the auction based on the information in the bid
    /// (update auction if higher bid received and send coin back for
    /// bids that are too low). This is executed by the auctioneer.
    public(script) fun update_auction<T: key + store>(
        auction: &mut Auction<T>, bid: Bid, ctx: &mut TxContext
    ) {
        let Bid { id, bidder, auction_id, bid: balance } = bid;
        assert!(AuctionLib::auction_id(auction) == &auction_id, EWrongAuction);
        AuctionLib::update_auction(auction, bidder, balance, ctx);

        ID::delete(id);
    }

    /// Ends the auction - transfers item to the currently highest
    /// bidder or to the original owner if no bids have been
    /// placed. This is executed by the auctioneer.
    public(script) fun end_auction<T: key + store>(
        auction: Auction<T>, ctx: &mut TxContext
    ) {
        AuctionLib::end_and_destroy_auction(auction, ctx);
    }
}
