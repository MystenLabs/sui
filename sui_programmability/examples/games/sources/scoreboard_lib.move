// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module games::scoreboard_lib {
    use sui::utf8;
    use std::vector;
    // use std::debug;
    use sui::id::{Self, ID, VersionedID};
    use sui::transfer;
    use sui::tx_context::{Self,TxContext};
    use std::option::{Self, Option};

    /// Stores information about a score.
    // FIXME drop?
    struct Score has store, drop {
        /// Coin representing the current (highest) bid.
        score: u64,
        /// 
        score_obj: ID,
        /// Address of the highest bidder.
        player: address,
    }

    public fun new_score(score: u64, score_obj: ID, player: address): Score {
        Score {
            score: score,
            score_obj: score_obj,
            player: player,
        }
    }

    public fun score(score: &Score): u64 {
        score.score
    }

    /// Maintains the state of the scoreboard owned by a trusted scoreboard owner.
    struct Scoreboard has key {
        id: VersionedID,
        name: utf8::String, 
        description: utf8::String, 
        /// The maximum number of highest score to store.
        capacity: u8,

        /// A vector of top scores, in descending order. Break tie by time, 
        /// i.e. a score that is recorded earlier is considered higher
        /// than a score with the same value but recorded later
        top_scores: vector<Score>,
    }

    public fun scoreboard_id(scoreboard: &Scoreboard): &ID {
        id::inner(&scoreboard.id)
    }

    public fun capacity(scoreboard: &Scoreboard): u8 {
        scoreboard.capacity
    }

    public fun top_scores(scoreboard: &Scoreboard): &vector<Score> {
        &scoreboard.top_scores
    }

    /// Creates a scoreboard.
    public fun create(
        name: vector<u8>, description: vector<u8>, capacity: u8, ctx: &mut TxContext
    ) {
        assert!(capacity > 0, 1);

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
    public fun record_score(
        scoreboard: &mut Scoreboard,
        score: Score,
        _ctx: &mut TxContext
    ): Option<u8> {
        bisect(&mut scoreboard.top_scores, score, scoreboard.capacity)
    }

    public fun bisect(top_scores: &mut vector<Score>, candidate: Score, capacity: u8): Option<u8> {
        if (vector::is_empty(top_scores)) {
            vector::push_back(top_scores, candidate);
            return option::some(0)
        };
        let candidate_score = score(&candidate);
        let left: u64 = 0;
        let right: u64 = vector::length(top_scores) - 1;

        // desc order
        while (left + 1 < right) {
            // will not overflow
            let mid: u64 = (left + right) / 2;
            if (candidate_score > score(vector::borrow(top_scores, mid))) {
                right = mid;
            } else {
                left = mid + 1;
            };
        };
        let left_score: u64 = score(vector::borrow(top_scores, left));
        let right_score: u64 = score(vector::borrow(top_scores, right));
        let pos: Option<u8> = option::none();
        if (candidate_score > left_score) {
            option::fill(&mut pos, (left as u8));
            insert_at(top_scores, candidate, left);
        } else if (candidate_score > right_score) {
            option::fill(&mut pos, (right as u8));
            insert_at(top_scores, candidate, right); 
        } else if (candidate_score <= right_score && right + 1 < vector::length(top_scores)) {
            option::fill(&mut pos, ((right+1) as u8));
            insert_at(top_scores, candidate, right + 1);
        };
        if (vector::length(top_scores) > (capacity as u64)) {
            vector::pop_back(top_scores);
        };
        pos
    }

    public fun insert_at(top_scores: &mut vector<Score>, candidate: Score, index: u64) {
        // debug::print(top_scores);
        // debug::print(&candidate);
        // debug::print(&index);
        vector::push_back(top_scores, candidate);
        // debug::print(top_scores);
        let i = vector::length(top_scores) - 1;
        while (i > index) {
            // debug::print(&i);
            vector::swap(top_scores, i, i - 1);
            i = i - 1;
        };
    }

    // struct ScoreboardUpdateEvent has copy, drop {

    // }
}

#[test_only]
module games::scoreboard_libTests {
    use games::scoreboard_lib::{Self, Scoreboard};
    use sui::test_scenario::{Self, next_tx, ctx};
    use std::vector;
    use sui::id;
    use std::option;
    use std::debug;


    const CREATOR: address = @0xBEEF;
    const USER1: address = @0xBEE1;
    const USER2: address = @0xBEE2;

    #[test]
    fun test_basic() {
        let scenario = &mut test_scenario::begin(&CREATOR);
        {
            scoreboard_lib::create(b"test", b"test description", 3, ctx(scenario));
        }; next_tx(scenario, &CREATOR);

        {
            let scoreboard = test_scenario::take_shared<Scoreboard>(scenario);
            let scoreboard_ref = test_scenario::borrow_mut(&mut scoreboard);
            let scoreboard_id = *id::id(scoreboard_ref);
            assert!(scoreboard_lib::capacity(scoreboard_ref) == 3, 1);
            let top_scores = scoreboard_lib::top_scores(scoreboard_ref);
            assert!(vector::is_empty(top_scores), 2);

            let pos = scoreboard_lib::record_score(
                scoreboard_ref, 
                scoreboard_lib::new_score(666, scoreboard_id, USER1),
                ctx(scenario)
            );
            debug::print(&pos);
            assert!(pos == option::some(0), 3);
            test_scenario::return_shared(scenario, scoreboard);
        }; next_tx(scenario, &USER1); 
        
        {
            let scoreboard = test_scenario::take_shared<Scoreboard>(scenario);
            let scoreboard_ref = test_scenario::borrow_mut(&mut scoreboard);
            let scoreboard_id = *id::id(scoreboard_ref);
            let top_scores = scoreboard_lib::top_scores(scoreboard_ref);
            let expected_top_scores = vector[scoreboard_lib::new_score(666, scoreboard_id, USER1)];
            assert!(top_scores == &expected_top_scores, 4);

            let pos = scoreboard_lib::record_score(
                scoreboard_ref, 
                scoreboard_lib::new_score(10086, scoreboard_id, USER2),
                ctx(scenario)
            );
            assert!(pos == option::some(0), 3);
            test_scenario::return_shared(scenario, scoreboard);
        }; next_tx(scenario, &USER2); 

        {
            let scoreboard = test_scenario::take_shared<Scoreboard>(scenario);
            let scoreboard_ref = test_scenario::borrow_mut(&mut scoreboard);
            let scoreboard_id = *id::id(scoreboard_ref);
            let top_scores = scoreboard_lib::top_scores(scoreboard_ref);
            let expected_top_scores = vector[
                scoreboard_lib::new_score(10086, scoreboard_id, USER2),
                scoreboard_lib::new_score(666, scoreboard_id, USER1)
            ];
            assert!(top_scores == &expected_top_scores, 4);

            let pos = scoreboard_lib::record_score(
                scoreboard_ref, 
                scoreboard_lib::new_score(10000, scoreboard_id, USER1),
                ctx(scenario)
            );
            assert!(pos == option::some(1), 3);
            test_scenario::return_shared(scenario, scoreboard);
        }; next_tx(scenario, &USER1); 

        {
            let scoreboard = test_scenario::take_shared<Scoreboard>(scenario);
            let scoreboard_ref = test_scenario::borrow_mut(&mut scoreboard);
            let scoreboard_id = *id::id(scoreboard_ref);
            let top_scores = scoreboard_lib::top_scores(scoreboard_ref);
            let expected_top_scores = vector[
                scoreboard_lib::new_score(10086, scoreboard_id, USER2),
                scoreboard_lib::new_score(10000, scoreboard_id, USER1),
                scoreboard_lib::new_score(666, scoreboard_id, USER1)
            ];
            assert!(top_scores == &expected_top_scores, 4);

            let pos = scoreboard_lib::record_score(
                scoreboard_ref, 
                scoreboard_lib::new_score(665, scoreboard_id, USER2),
                ctx(scenario)
            );
            assert!(pos == option::none(), 3);
            test_scenario::return_shared(scenario, scoreboard);
        }; next_tx(scenario, &USER2); 

        {
            let scoreboard = test_scenario::take_shared<Scoreboard>(scenario);
            let scoreboard_ref = test_scenario::borrow_mut(&mut scoreboard);
            let scoreboard_id = *id::id(scoreboard_ref);
            let top_scores = scoreboard_lib::top_scores(scoreboard_ref);
            let expected_top_scores = vector[
                scoreboard_lib::new_score(10086, scoreboard_id, USER2),
                scoreboard_lib::new_score(10000, scoreboard_id, USER1),
                scoreboard_lib::new_score(666, scoreboard_id, USER1)
            ];
            assert!(top_scores == &expected_top_scores, 4);
            let pos = scoreboard_lib::record_score(
                scoreboard_ref, 
                scoreboard_lib::new_score(666, scoreboard_id, USER2),
                ctx(scenario)
            );    
            assert!(pos == option::none(), 3);
            test_scenario::return_shared(scenario, scoreboard);
        }; next_tx(scenario, &USER2);

        {
            let scoreboard = test_scenario::take_shared<Scoreboard>(scenario);
            let scoreboard_ref = test_scenario::borrow_mut(&mut scoreboard);
            let scoreboard_id = *id::id(scoreboard_ref);
            let top_scores = scoreboard_lib::top_scores(scoreboard_ref);
            let expected_top_scores = vector[
                scoreboard_lib::new_score(10086, scoreboard_id, USER2),
                scoreboard_lib::new_score(10000, scoreboard_id, USER1),
                scoreboard_lib::new_score(666, scoreboard_id, USER1)
            ];
            assert!(top_scores == &expected_top_scores, 4);
            test_scenario::return_shared(scenario, scoreboard);
        };
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    fun test_invalid_capacity() {
        let scenario = &mut test_scenario::begin(&CREATOR);
        {
            scoreboard_lib::create(b"test", b"test description", 0, ctx(scenario));
        }; next_tx(scenario, &CREATOR);
    }
}