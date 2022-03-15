/// This is a helper module for implementing two versions of an
/// English auction (https://en.wikipedia.org/wiki/English_auction),
/// one using single-owner objects only and the other using shared
/// objects.
module DeFi::AuctionUtils {
    use Std::Option::{Self, Option};

    use Sui::Coin::{Self, Coin};
    use Sui::GAS::GAS;
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self,TxContext};

    friend DeFi::Auction;
    friend DeFi::AuctionV2;

    /// Stores information about an auction bid.
    struct BidData has store {
        /// Coin representing the current (highest) bid.
        funds: Coin<GAS>,
        /// Address of the highest bidder.
        highest_bidder: address,
    }

    /// Maintains the state of the auction owned by a trusted
    /// auctioneer.
    struct Auction<T:  key + store> has key {
        id: VersionedID,
        /// Item to be sold.
        to_sell: T,
        /// Owner of the time to be sold.
        owner: address,
        /// Data representing the highest bid (starts with no bid)
        bid_data: Option<BidData>,
    }

    public(friend) fun auction_id<T: key + store>(auction: &Auction<T>): &ID {
        ID::inner(&auction.id)
    }

    public(friend) fun auction_owner<T: key + store>(auction: &Auction<T>): address {
        auction.owner
    }

    /// Creates an auction. This is executed by the owner of the asset to be
    /// auctioned.
    public(friend) fun create_auction<T: key + store>(id: VersionedID, to_sell: T, ctx: &mut TxContext): Auction<T> {
        // A question one might asked is how do we know that to_sell
        // is owned by the caller of this entry function and the
        // answer is that it's checked by the runtime.
        Auction<T> {
            id,
            to_sell,
            owner: TxContext::sender(ctx),
            bid_data: Option::none(),
        }
    }

    /// Updates the auction based on the information in the bid
    /// (update auction if higher bid received and send coin back for
    /// bids that are too low).
    public fun update_auction<T: key + store>(auction: &mut Auction<T>, bidder: address, coin: Coin<GAS>) {
        if (Option::is_none(&auction.bid_data)) {
            // first bid
            let bid_data = BidData {
                funds: coin,
                highest_bidder: bidder,
            };
            Option::fill(&mut auction.bid_data, bid_data);
        } else {
            let prev_bid_data = Option::borrow(&auction.bid_data);
            if (Coin::value(&coin) > Coin::value(&prev_bid_data.funds)) {
                // a bid higher than currently highest bid received
                let new_bid_data = BidData {
                    funds: coin,
                    highest_bidder: bidder
                };
                // update auction to reflect highest bid
                let BidData { funds, highest_bidder } = Option::swap(&mut auction.bid_data, new_bid_data);
                // transfer previously highest bid to its bidder
                Coin::transfer(funds, highest_bidder);
            } else {
                // a bid is too low - return funds to the bidder
                Coin::transfer(coin, bidder);
            }
        }
    }


    /// Ends the auction - transfers item to the currently highest
    /// bidder or to the original owner if no bids have been placed.
    public fun end_auction<T: key + store>(auction: Auction<T>) {
        let Auction { id, to_sell, owner, bid_data } = auction;
        ID::delete(id);

        if (Option::is_some<BidData>(&bid_data)) {
            // bids have been placed - send funds to the original item
            // owner and the item to the highest bidder
            let BidData { funds, highest_bidder } = Option::extract(&mut bid_data);
            Transfer::transfer(funds, owner);
            Transfer::transfer(to_sell, highest_bidder);
        } else {
            // no bids placed - send the item back to the original owner
            Transfer::transfer(to_sell, owner);
        };
        // there is no bid data left regardless of the result, but the
        // option still needs to be destroyed
        Option::destroy_none(bid_data);
    }

}
