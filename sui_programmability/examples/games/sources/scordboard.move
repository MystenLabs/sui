// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module games::sui_2048 {
    use sui::id::{Self, ID, VersionedID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::option::{Self, Option};
    use std::vector;
    // use games::scoreboard_lib::{Self, };

    /// Immutable "soulbound" score
    struct Sui2048Score has key {
        id: VersionedID,
        score: u64,
        player: address,
    }

    struct Sui2048ScoreHistory has key {
        id: VersionedID,
        best_score: Option<u64>,
        best_score_id: Option<ID>,
        /// Past score objects. 
        // FIXME will ID's player mismatch?
        score_ids: vector<ID>,
    }

    // TODO: how to make sure each address only has one history?
    public entry fun create_history(ctx: &mut TxContext) {
        let history = Sui2048ScoreHistory {
            id: tx_context::new_id(ctx),
            best_score: option::none(),
            best_score_id: option::none(),
            score_ids: vector::empty(),
        };
        transfer::transfer(history, tx_context::sender(ctx));
    }

    public entry fun report_score(
        // FIXMe test A can't touch B's history
        history: &mut Sui2048ScoreHistory, new_score: u64, ctx: &mut TxContext
    ) {
        let score_obj = Sui2048Score {
            id: tx_context::new_id(ctx),
            score: new_score,
            player: tx_context::sender(ctx),
        };
        let score_obj_id = *id::inner(&score_obj.id);
        if (option::is_none(&history.best_score) || new_score > *option::borrow(&history.best_score)) {
            option::swap_or_fill(&mut history.best_score, new_score);
            option::swap_or_fill(&mut history.best_score_id, score_obj_id);
        };
        transfer::freeze_object(score_obj);
        vector::push_back(&mut history.score_ids, score_obj_id);

        // let Bid { id, bidder, auction_id, bid: balance } = bid;
        // assert!(auction_lib::auction_id(auction) == &auction_id, EWrongAuction);
        // auction_lib::update_auction(auction, bidder, balance, ctx);

        // id::delete(id);
    }

    // /// Ends the auction - transfers item to the currently highest
    // /// bidder or to the original owner if no bids have been
    // /// placed. This is executed by the auctioneer.
    // public entry fun end_auction<T: key + store>(
    //     auction: Auction<T>, ctx: &mut TxContext
    // ) {
    //     auction_lib::end_and_destroy_auction(auction, ctx);
    // }

}