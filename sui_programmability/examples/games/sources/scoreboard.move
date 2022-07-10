// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module games::scoreboard {
    use sui::utf8;
    use std::vector;
    use sui::id::{Self, ID, VersionedID};
    use sui::transfer;
    use sui::tx_context::{Self,TxContext};

    // friend nfts::auction;
    // friend nfts::shared_auction;

    /// Stores information about an auction bid.
    struct Score has store {
        /// Coin representing the current (highest) bid.
        score: u64,
        /// 
        score_obj: ID,
        /// Address of the highest bidder.
        player: address,
    }

    /// Maintains the state of the scoreboard owned by a trusted scoreboard owner.
    struct Scoreboard has key {
        id: VersionedID,
        name: utf8::String, 
        description: utf8::String, 
        /// The maximum number of highest score to store.
        capacity: u8,

        // /// Owner of the time to be sold.
        // owner: address,

        /// A vector of top scores, in descending order. Break tie by time, 
        /// i.e. a score that is recorded earlier is considered higher
        /// than a score with the same value but recorded later
        top_scores: vector<Score>,
    }

    public fun scoreboard_id(scoreboard: &Scoreboard): &ID {
        id::inner(&scoreboard.id)
    }

    /// Creates a scoreboard.
    public entry fun create_scoreboard(
        name: vector<u8>, description: vector<u8>, capacity: u8, ctx: &mut TxContext
    ) {
        assert!(capacity > 0, 1);

        // A question one might asked is how do we know that to_sell
        // is owned by the caller of this entry function and the
        // answer is that it's checked by the runtime.
        let scoreboard = Scoreboard {
            id: tx_context::new_id(ctx),
            name: utf8::string_unsafe(name),
            description: utf8::string_unsafe(description),
            capacity: capacity,
            top_scores: vector::empty(),
        };
        transfer::share_object(scoreboard);
    }

    /// Updates the auction based on the information in the bid
    /// (update auction if higher bid received and send coin back for
    /// bids that are too low).
    public entry fun record_score(
        scoreboard: &mut Scoreboard,
        score: Score,
        ctx: &mut TxContext,
    ) {
        // TODO separate a lib file to construct Score
        // TODO verify Score, check input is a certain type
        bisect(&mut scoreboard.top_scores, score, scoreboard.capacity);
    }

    public fun bisect(top_scores: &mut vector<Score>, candidate: Score, capacity: u8) {
        if (vector::is_empty(top_scores)) {
            vector::push_back(scoreboard.top_scores, candidate);
        }
        let left: u64 = 0;
        let right: u64 = vector::length(top_scores) - 1;

        // desc order
        while (left + 1 < right) {
            // will not overflow
            let mid: u64 = (left + right) / 2;
            if (candidate.score > vector::borrow(top_scores, mid).score) {
                right = mid;
            } else {
                left = mid + 1;
            }
        };
        let left_score = vector::borrow(top_scores, left).score;
        let right_score = vector::borrow(top_scores, right).score;
        if (candidate.score > left_score) {
            insert_at(top_scores, candidate, left);
        } else if (candidate.score <= right_score) {
            insert_at(top_scores, candidate, right + 1);
        } else if (candidate.score > right_score) {
            insert_at(top_scores, candidate, right); 
        }
        if (vector::length(top_scores) > capacity) {
            vector::pop_back(top_scores);
        }
    }

    public fun insert_at(top_scores: &mut vector<Score>, candidate: Score, index: u8) {
        top_scores.push_back(candidate);
        let i = vector::length(top_scores);
        while (i > index) {
            vector::swap(top_scores, i, i - 1);
            i = i - 1;
        }
    }
}