// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module frenemies::frenemies_tests {
    use frenemies::frenemies::{Self, Scorecard};
    use frenemies::leaderboard::{Self, Leaderboard};
    use frenemies::registry::{Self, Registry};
    use std::string;
    use std::vector;
    use sui::address;
    use sui::coin;
    use sui::governance_test_utils as gtu;
    use sui::sui_system::{Self, SuiSystemState};
    use sui::test_scenario::{Self as ts, Scenario};
    use sui::tx_context;

    const FRIEND: u8 = 0;
    const NEUTRAL: u8 = 1;
    const ENEMY: u8 = 2;
    const GOAL_COMPLETION_POINTS: u16 = 10;

    #[test_only]
    public fun init_test(validators: vector<address>, players: vector<address>): Scenario {
        use sui::governance_test_utils as gtu;

        // set up system state
        let scenario = ts::begin(@0x0);
        gtu::set_up_sui_system_state(validators, &mut scenario);
        frenemies::init_for_testing(ts::ctx(&mut scenario));
        old_frenemies::registry::init_for_testing(ts::ctx(&mut scenario));

        // each player registers for the game
        let i = 0;
        while (i < vector::length(&players)) {
            let player = *vector::borrow(&players, i);
            ts::next_tx(&mut scenario, player);
            {
                let system_state = ts::take_shared<SuiSystemState>(&mut scenario);
                let registry = ts::take_shared<Registry>(&mut scenario);
                let old_registry = ts::take_shared<old_frenemies::registry::Registry>(&mut scenario);
                let tx_context = ts::ctx(&mut scenario);
                frenemies::register(address::to_string(player), &mut registry, &mut old_registry, &mut system_state, tx_context);
                assert!(registry::num_players(&registry) == i + 1, 0);
                ts::return_shared(system_state);
                ts::return_shared(registry);
                ts::return_shared(old_registry);
            };
            i = i + 1
        };
        scenario
    }

    #[test]
    fun double_register_ok() {
        // registering twice from same address + different names is fine, but you should get the same assignment
        let validators = vector[@0x1];

        let scenario_val = init_test(copy validators, validators);
        let scenario = &mut scenario_val;
        ts::next_tx(scenario, @0xb0b);
        {
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            let registry = ts::take_shared<Registry>(scenario);
            let old_registry = ts::take_shared<old_frenemies::registry::Registry>(scenario);
            let tx_context = ts::ctx(scenario);
            frenemies::register(string::utf8(b"bob"), &mut registry, &mut old_registry, &mut system_state, tx_context);
            frenemies::register(string::utf8(b"alice"), &mut registry, &mut old_registry, &mut system_state, tx_context);
            ts::return_shared(system_state);
            ts::return_shared(registry);
            ts::return_shared(old_registry);
        };
        let effects = ts::next_tx(scenario, @0xb0b);
        let created = ts::created(&effects);
        assert!(vector::length(&created) == 4, 0);
        let card1 = ts::take_from_sender_by_id<Scorecard>(scenario, *vector::borrow(&created, 1));
        let card2 = ts::take_from_sender_by_id<Scorecard>(scenario, *vector::borrow(&created, 3));
        assert!(frenemies::assignment(&card1) == frenemies::assignment(&card2), 0);
        ts::return_to_sender(scenario, card1);
        ts::return_to_sender(scenario, card2);
        ts::end(scenario_val);
    }

    #[expected_failure(abort_code = frenemies::frenemies::EScoreNotYetAvailable)]
    #[test]
    fun score_in_start_epoch() {
        // attempting to get a score during the start epoch should fail
        let validators = vector[@0x1];
        let scenario_val = init_test(copy validators, validators);
        let scenario = &mut scenario_val;
        ts::next_tx(scenario, @0x1);
        {
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            let scorecard = ts::take_from_sender<Scorecard>(scenario);
            let leaderboard = ts::take_shared<Leaderboard>(scenario);
            frenemies::update(&mut scorecard, &mut system_state, &mut leaderboard, ts::ctx(scenario));
            ts::return_to_sender(scenario, scorecard);
            ts::return_shared(system_state);
            ts::return_shared(leaderboard)
        };
        let effects = ts::end(scenario_val);
        // should emit an event for each scorecard update
        assert!(ts::num_user_events(&effects) == 1, 0)
    }

    #[expected_failure(abort_code = frenemies::frenemies::EScoreNotYetAvailable)]
    #[test]
    fun double_update() {
        // attempting to update a scorecard twice should fail
        let validators = vector[@0x1];
        let scenario_val = init_test(copy validators, validators);
        let scenario = &mut scenario_val;
        gtu::advance_epoch(scenario);
        ts::next_tx(scenario, @0x1);
        {
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            let scorecard = ts::take_from_sender<Scorecard>(scenario);
            let leaderboard = ts::take_shared<Leaderboard>(scenario);
            frenemies::update(&mut scorecard, &mut system_state, &mut leaderboard, ts::ctx(scenario));
            frenemies::update(&mut scorecard, &mut system_state, &mut leaderboard, ts::ctx(scenario));
            ts::return_to_sender(scenario, scorecard);
            ts::return_shared(system_state);
            ts::return_shared(leaderboard)
        };
        ts::end(scenario_val);
    }

    #[test]
    fun basic_score() {
        let validators = vector[@0x1, @0x2, @0x3];

        let scenario_val = init_test(copy validators, validators);
        let scenario = &mut scenario_val;
        let scorecard = ts::take_from_address<Scorecard>(scenario, @0x1);
        // hardcode assignment for convenience
        frenemies::set_assignment_for_testing(&mut scorecard, @0x1, FRIEND, tx_context::epoch(ts::ctx(scenario)));
        ts::return_to_address(@0x1, scorecard);

        ts::next_tx(scenario, @0x1);
        {
            let scorecard = ts::take_from_sender<Scorecard>(scenario);
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            // stake 100 with 1, 50 with 2, nothing with 3
            sui_system::request_add_stake(
                &mut system_state, coin::mint_for_testing(100, ts::ctx(scenario)), @0x1, ts::ctx(scenario)
            );
            sui_system::request_add_stake(
                &mut system_state, coin::mint_for_testing(50, ts::ctx(scenario)), @0x2, ts::ctx(scenario)
            );
            ts::return_to_sender(scenario, scorecard);
            ts::return_shared(system_state)
        };
        // advance epoch so player can get a score
        gtu::advance_epoch(scenario);
        ts::next_tx(scenario, @0x1);
        {
            let scorecard = ts::take_from_sender<Scorecard>(scenario);
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            let leaderboard = ts::take_shared<Leaderboard>(scenario);
            frenemies::update(&mut scorecard, &mut system_state, &mut leaderboard, ts::ctx(scenario));
            assert!(frenemies::score(&scorecard) == GOAL_COMPLETION_POINTS, 0);
            assert!(frenemies::participation(&scorecard) == 1, 0);
            assert!(
                leaderboard::top_scores(&leaderboard) == &vector[leaderboard::score_for_testing(address::to_string(@0x1), GOAL_COMPLETION_POINTS, 1)],
                0
            );
            // hardcode assignment to 3, stake 200 with 3
            frenemies::set_assignment_for_testing(&mut scorecard, @0x3, FRIEND, tx_context::epoch(ts::ctx(scenario)));
            sui_system::request_add_stake(
                &mut system_state, coin::mint_for_testing(200, ts::ctx(scenario)), @0x3, ts::ctx(scenario)
            );

            ts::return_to_sender(scenario, scorecard);
            ts::return_shared(system_state);
            ts::return_shared(leaderboard)
        };
        // now, validator 3 has gone from last to first. player should be rewarded accordingly. scoreboard remains the same
        gtu::advance_epoch(scenario);
        ts::next_tx(scenario, @0x1);
        {
            let scorecard = ts::take_from_sender<Scorecard>(scenario);
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            let leaderboard = ts::take_shared<Leaderboard>(scenario);
            frenemies::update(&mut scorecard, &mut system_state, &mut leaderboard, ts::ctx(scenario));
            let difficulty = 2; // moved 2 places, last to first
            let new_score = (GOAL_COMPLETION_POINTS * 2) + difficulty;
            assert!(frenemies::score(&scorecard) == new_score, 0);
            assert!(frenemies::participation(&scorecard) == 2, 0);
            // leaderboard should still only have one entry, but score and participation are updated
            assert!(
                leaderboard::top_scores(&leaderboard) == &vector[leaderboard::score_for_testing(address::to_string(@0x1), new_score, 2)],
                0
            );
            ts::return_to_sender(scenario, scorecard);
            ts::return_shared(system_state);
            ts::return_shared(leaderboard)
        };
        ts::end(scenario_val);
    }

    #[test]
    fun late_score() {
        // if you have an assignment in epoch N, but neglect to call update in N + 1, you should still be able to get a score
         let validators = vector[@0x1, @0x2, @0x3];

        let scenario_val = init_test(copy validators, validators);
        let scenario = &mut scenario_val;
        let scorecard1 = ts::take_from_address<Scorecard>(scenario, @0x1);
        let scorecard2 = ts::take_from_address<Scorecard>(scenario, @0x2);
        // hardcode assignments for player 1 and 2
        frenemies::set_assignment_for_testing(&mut scorecard1, @0x1, FRIEND, tx_context::epoch(ts::ctx(scenario)));
        frenemies::set_assignment_for_testing(&mut scorecard2, @0x1, FRIEND, tx_context::epoch(ts::ctx(scenario)));
        ts::return_to_address(@0x1, scorecard1);
        ts::return_to_address(@0x2, scorecard2);

        // set player 1 and 2 up for a win
        ts::next_tx(scenario, @0x1);
        {
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            // stake 100 with 1 to get a win
            sui_system::request_add_stake(
                &mut system_state, coin::mint_for_testing(100, ts::ctx(scenario)), @0x1, ts::ctx(scenario)
            );
            ts::return_shared(system_state)
        };
        // player 1 calls update() in this epoch, but player 2 does not
        gtu::advance_epoch(scenario);
        ts::next_tx(scenario, @0x1);
        {
            let scorecard = ts::take_from_sender<Scorecard>(scenario);
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            let leaderboard = ts::take_shared<Leaderboard>(scenario);
            frenemies::update(&mut scorecard, &mut system_state, &mut leaderboard, ts::ctx(scenario));
             assert!(
                leaderboard::top_scores(&leaderboard) == &vector[
                    leaderboard::score_for_testing(address::to_string(@0x1), GOAL_COMPLETION_POINTS, 1)
                ],
                0
            );
            // validator 1 is no longer in the lead
            sui_system::request_add_stake(
                &mut system_state, coin::mint_for_testing(200, ts::ctx(scenario)), @0x2, ts::ctx(scenario)
            );
            ts::return_to_sender(scenario, scorecard);
            ts::return_shared(system_state);
            ts::return_shared(leaderboard)
        };
        // now, player 2 calls update(). they should still get the same score/participation as player 1, and their assignment should be for the next epoch
        gtu::advance_epoch(scenario);
        ts::next_tx(scenario, @0x2);
        {
            let scorecard = ts::take_from_sender<Scorecard>(scenario);
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            let leaderboard = ts::take_shared<Leaderboard>(scenario);
            frenemies::update(&mut scorecard, &mut system_state, &mut leaderboard, ts::ctx(scenario));
            assert!(frenemies::score(&scorecard) == GOAL_COMPLETION_POINTS, 0);
            assert!(frenemies::participation(&scorecard) == 1, 0);
            assert!(
                leaderboard::top_scores(&leaderboard) == &vector[
                    leaderboard::score_for_testing(address::to_string(@0x1), GOAL_COMPLETION_POINTS, 1),
                    leaderboard::score_for_testing(address::to_string(@0x2), GOAL_COMPLETION_POINTS, 1)
                ],
                0
            );
            assert!(frenemies::epoch(&scorecard) == tx_context::epoch(ts::ctx(scenario)) + 1, 0);
            ts::return_to_sender(scenario, scorecard);
            ts::return_shared(system_state);
            ts::return_shared(leaderboard)
        };
        ts::end(scenario_val);
    }

    #[test]
    fun basic_e2e() {
        let v1 = @0x1;
        let v2 = @0x2;
        let v3 = @0x3;
        let v4 = @0x4;
        let validators = vector[v1, v2, v3, v4];

        let scenario_val = init_test(copy validators, validators);
        let scenario = &mut scenario_val;
        // hardcode some assignments for convenience
        let v2_scorecard = ts::take_from_address<Scorecard>(scenario, @0x2);
        //let v4_scorecard = ts::take_from_address<Scorecard>(scenario, v4);
        frenemies::set_assignment_for_testing(&mut v2_scorecard, @0x2, FRIEND, tx_context::epoch(ts::ctx(scenario)));
        //frenemies::set_assignment_for_testing(&mut v4_scorecard, @0x4, FRIEND, tx_context::epoch(ts::ctx(scenario)));
        ts::return_to_address(@0x2, v2_scorecard);
        //ts::return_to_address(@0x4, v4_scorecard);

        ts::next_tx(scenario, v2);
        {
            let scorecard = ts::take_from_sender<Scorecard>(scenario);
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            let coin = coin::mint_for_testing(10, ts::ctx(scenario));
            sui_system::request_add_stake(
                &mut system_state, coin, frenemies::validator(&scorecard), ts::ctx(scenario)
            );
            ts::return_to_sender(scenario, scorecard);
            ts::return_shared(system_state)
        };
        // advance epoch so player 2 can get a score
        gtu::advance_epoch(scenario);
        ts::next_tx(scenario, v2);
        // user 2 updates scorecard
        {
            let scorecard = ts::take_from_sender<Scorecard>(scenario);
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            let leaderboard = ts::take_shared<Leaderboard>(scenario);
            frenemies::update(&mut scorecard, &mut system_state, &mut leaderboard, ts::ctx(scenario));
            assert!(frenemies::score(&scorecard) == GOAL_COMPLETION_POINTS, 0);
            assert!(frenemies::participation(&scorecard) == 1, 0);
            assert!(
                leaderboard::top_scores(&leaderboard) == &vector[leaderboard::score_for_testing(address::to_string(v2), GOAL_COMPLETION_POINTS, 1)],
                0
            );

            ts::return_to_sender(scenario, scorecard);
            ts::return_shared(system_state);
            ts::return_shared(leaderboard)
        };
        ts::next_tx(scenario, v4);
        // user 4 updates scorecard
        {
            let scorecard = ts::take_from_sender<Scorecard>(scenario);
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            let leaderboard = ts::take_shared<Leaderboard>(scenario);
            // hardcode assignment for convenience
            frenemies::set_assignment_for_testing(&mut scorecard, @0x4, FRIEND, 0);
            frenemies::update(&mut scorecard, &mut system_state, &mut leaderboard, ts::ctx(scenario));
            // user 4 did not score because their assignment was not met
            assert!(frenemies::score(&scorecard) == 0, 0);
            // user 4's participation is recorded
            assert!(frenemies::participation(&scorecard) == 1, 0);
            // leaderboard should continue to have one entry because user 4 did not score
            assert!(vector::length(leaderboard::top_scores(&leaderboard)) == 1, 0);

            ts::return_to_sender(scenario, scorecard);
            ts::return_shared(system_state);
            ts::return_shared(leaderboard)
        };
        ts::next_tx(scenario, v1);
        // user 3 updates scorecard
        {
            let scorecard = ts::take_from_sender<Scorecard>(scenario);
            let system_state = ts::take_shared<SuiSystemState>(scenario);
            let leaderboard = ts::take_shared<Leaderboard>(scenario);
            frenemies::set_assignment_for_testing(&mut scorecard, @0x3, ENEMY, 0);
            frenemies::update(&mut scorecard, &mut system_state, &mut leaderboard, ts::ctx(scenario));
            // user 3 scored
            assert!(frenemies::score(&scorecard) == GOAL_COMPLETION_POINTS, 0);
            // user 3's participation is recorded
            assert!(frenemies::participation(&scorecard) == 1, 0);
            // leaderboard should now have two entries
            assert!(vector::length(leaderboard::top_scores(&leaderboard)) == 2, 0);

            ts::return_to_sender(scenario, scorecard);
            ts::return_shared(system_state);
            ts::return_shared(leaderboard)
        };
        ts::end(scenario_val);
    }
}
