// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This is an implementation of an English auction
/// (https://en.wikipedia.org/wiki/English_auction) using shared
/// objects. There are types of participants:
/// - owner - this is the original owner of an item that is sold at an
/// auction; the owner creates an auction and ends it the time of her
/// choice
/// - bidders - these are parties interested in purchasing items sold
/// at an auction; similarly to the owner they have access to the
/// auction object and can submit bids to change its state

/// A typical lifetime of an auction looks as follows:
/// - auction is created by the owner and shared with the bidders
/// - bidders submit bids to try out-biding one another
///   - if a submitted bid is higher than the current bid (initially
///   there is no bid), the auction is updated with the current bid
///   and funds representing previous highest bid are sent to the
///   original owner
///   - otherwise (bid is too low) the bidder's funds are sent back to
///   the bidder and the auction remains unchanged
/// - the owner eventually ends the auction
///   - if no bids were received, the item goes back to the owner
///   - otherwise the funds accumulated in the auction go to the owner
///   and the item goes to the bidder that won the auction

module nfts::shared_auction {
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};

    use nfts::auction_lib::{Self, Auction};

    // Error codes.

    /// An attempt to end auction by a different user than the owner
    const EWrongOwner: u64 = 1;

    // Entry functions.

    /// Creates an auction. This is executed by the owner of the asset
    /// to be auctioned.
    public entry fun create_auction<T: key + store >(to_sell: T, ctx: &mut TxContext) {
        let auction = auction_lib::create_auction(to_sell, ctx);
        auction_lib::share_object(auction);
    }

    /// Sends a bid to the auction. The result is either successful
    /// change of the auction state (if bid was high enough) or return
    /// of the funds (if the bid was too low). This is executed by a
    /// bidder.
    public entry fun bid<T: key + store>(
        coin: Coin<SUI>, auction: &mut Auction<T>, ctx: &mut TxContext
    ) {
        auction_lib::update_auction(
            auction,
            tx_context::sender(ctx),
            coin::into_balance(coin),
            ctx
        );
    }

    /// Ends the auction - transfers item to the currently highest
    /// bidder or back to the original owner if no bids have been
    /// placed. This is executed by the owner of the asset to be
    /// auctioned.
    public entry fun end_auction<T: key + store>(
        auction: &mut Auction<T>, ctx: &mut TxContext
    ) {
        let owner = auction_lib::auction_owner(auction);
        assert!(tx_context::sender(ctx) == owner, EWrongOwner);
        auction_lib::end_shared_auction(auction, ctx);
    }

}
