// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module scratch_off::test_game {
    
    use scratch_off::game::{Self, ConvenienceStore, ENoTicketsLeft, StoreCap, player_metadata,
     tickets_left, leaderboard, prize_pool_balance, leaderboard_players, get_target_player_metadata,
     tickets_claimed, amount_won, tickets_issued, init_for_testing, stock_store, Ticket};

    #[test_only] use sui::test_scenario::{Self, Scenario};
    #[test_only] use sui::coin::{mint_for_testing};
    use sui::test_scenario as ts;
    use std::vector;
    use sui::sui::SUI;

    const ALICE_ADDRESS: address = @0xAAAA;
    const BOB_ADDRESS: address = @0xBBBB;
    const OWNER_ADDRESS: address = @123;
    const MAX_LEADERBOARD_SIZE: u64 = 1;

    #[test_only]
    public fun setup_test_sui_store(
        scenario: &mut Scenario,
        creator: address,
        number_of_prizes: vector<u64>,
        value_of_prizes: vector<u64>,
        coin_amount: u64,
        max_leaderboard_size: u64,
    ) {
        ts::next_tx(scenario, creator);
        {
            init_for_testing(ts::ctx(scenario));
        };
        ts::next_tx(scenario, creator);
        {
            let store_cap: StoreCap = ts::take_from_sender(scenario);
            let store: ConvenienceStore = ts::take_shared(scenario);
            let coin = mint_for_testing<SUI>(coin_amount, ts::ctx(scenario));
            stock_store(
                &store_cap,
                &mut store,
                coin,
                number_of_prizes,
                value_of_prizes,
                b"key", // Fake public key
                max_leaderboard_size,
                ts::ctx(scenario)
            );
            ts::return_to_sender(scenario, store_cap);
            ts::return_shared(store);
        }
    }

    fun scenario(): Scenario { ts::begin(@0x1) }

    #[test]
    #[expected_failure(abort_code = ENoTicketsLeft)]
    fun test_no_more_tickets() {
        test_no_more_tickets_(scenario());
    }

    #[test]
    fun test_play_and_evaluate() {
        test_play_and_evaluate_(scenario());
    }

    #[test]
    fun test_play_and_evaluate_multiple_prizes() {
        test_play_and_evaluate_multiple_prizes_(scenario());
    }

    #[test]
    /// Checks that we never go past the sizing limit on leaderboard and that players are 
    /// evicted from the leaderboard as expected
    fun test_leaderboard_max_size() {
        test_leaderboard_max_size_(scenario());
    }

    #[test]
    fun test_player_metadata() {
        test_player_metadata_(scenario());
    }

    #[test]
    fun test_multiple_ticket_play_and_evaluate() {
        test_multiple_ticket_play_and_evaluate_(scenario());
    }

    #[test]
    #[expected_failure(abort_code = ENoTicketsLeft)]
    fun test_multiple_ticket_play_and_evaluate_failure() {
        test_multiple_ticket_play_and_evaluate_failure_(scenario());
    }

    fun test_multiple_ticket_play_and_evaluate_failure_(test: Scenario) {
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            let number_of_prizes = vector<u64>[1, 1, 1];
            let value_of_prizes = vector<u64>[1, 1, 1];
            setup_test_sui_store(
                &mut test,
                OWNER_ADDRESS,
                number_of_prizes,
                value_of_prizes,
                3,
                MAX_LEADERBOARD_SIZE
            )
        };
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            // Send a batch 3 tickets to alice
            let store: ConvenienceStore = ts::take_shared(&test);
            let store_cap: StoreCap = ts::take_from_sender(&test);
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, 4, ts::ctx(&mut test));
            ts::return_to_sender(&test, store_cap);
            ts::return_shared(store);
        };
        test_scenario::end(test);
    }

    fun test_multiple_ticket_play_and_evaluate_(test: Scenario) {
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            let number_of_prizes = vector<u64>[1, 1, 1];
            let value_of_prizes = vector<u64>[1, 1, 1];
            setup_test_sui_store(
                &mut test,
                OWNER_ADDRESS,
                number_of_prizes,
                value_of_prizes,
                3,
                MAX_LEADERBOARD_SIZE
            )
        };
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            // Send a batch 3 tickets to alice
            let store: ConvenienceStore = ts::take_shared(&test);
            let store_cap: StoreCap = ts::take_from_sender(&test);
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, 3, ts::ctx(&mut test));
            ts::return_to_sender(&test, store_cap);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, ALICE_ADDRESS);
        {

            let ticket: Ticket = ts::take_from_sender(&test);
            let store: ConvenienceStore = ts::take_shared(&test);

            assert!(prize_pool_balance(&store) == 3, 0);

            let id = game::evaluate_ticket(ticket, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing(id, b"test", &mut store, ts::ctx(&mut test));
            assert!(tickets_left(&store) == 0, 0);
            assert!(prize_pool_balance(&store) == 0, 0);
            ts::return_shared(store);
        };
        test_scenario::end(test);
    }

    fun test_player_metadata_(test: Scenario) {
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            let number_of_prizes = vector<u64>[1, 1, 1, 1];
            let value_of_prizes = vector<u64>[1, 1, 1, 1];
            setup_test_sui_store(
                &mut test,
                OWNER_ADDRESS,
                number_of_prizes,
                value_of_prizes,
                4,
                MAX_LEADERBOARD_SIZE
            )
        };
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            // Send 1 ticket to alice and 1 ticket to bob
            let store: ConvenienceStore = ts::take_shared(&test);
            let store_cap: StoreCap = ts::take_from_sender(&test);
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            game::send_ticket(&store_cap, BOB_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            game::send_ticket(&store_cap, BOB_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            game::send_ticket(&store_cap, BOB_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            let tickets_issued = tickets_issued(&store);
            assert!(tickets_issued == 4, 0);
            ts::return_to_sender(&test, store_cap);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, ALICE_ADDRESS);
        {
            let ticket: Ticket = ts::take_from_sender(&test);
            let store: ConvenienceStore = ts::take_shared(&test);
            let id = game::evaluate_ticket(ticket, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing(id, b"test", &mut store, ts::ctx(&mut test));
            let leaderboard = leaderboard(&store);
            let players_list = leaderboard_players(leaderboard);
            assert!(vector::length(&players_list) == 1, 0);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, BOB_ADDRESS);
        {
            let ticket: Ticket = ts::take_from_sender(&test);
            let store: ConvenienceStore = ts::take_shared(&test);
            let id = game::evaluate_ticket(ticket, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing(id, b"test", &mut store, ts::ctx(&mut test));
            let leaderboard = leaderboard(&store);
            let players_list = leaderboard_players(leaderboard);
            assert!(vector::length(&players_list) == 1, 0);
            // This is because owner should not change as bob would be the second person to climb to the leaderboard
            assert!(*vector::borrow(&players_list, 0) == ALICE_ADDRESS, 0);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, BOB_ADDRESS);
        {
            let ticket_2: Ticket = ts::take_from_sender(&test);
            let ticket_3: Ticket = ts::take_from_sender(&test);
            let store: ConvenienceStore = ts::take_shared(&test);
            let id = game::evaluate_ticket(ticket_2, &mut store, ts::ctx(&mut test));
            let id_2 = game::evaluate_ticket(ticket_3, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing(id, b"test", &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing(id_2, b"test", &mut store, ts::ctx(&mut test));

            let leaderboard = leaderboard(&store);
            let players_list = leaderboard_players(leaderboard);
            assert!(vector::length(&players_list) == 1, 0);
            // This is because owner should not change
            assert!(*vector::borrow(&players_list, 0) == BOB_ADDRESS, 0);

            // Assert that bob has 3 sui win and alice has 1 sui win
            let player_table = player_metadata(&store);
            let bob_meta = get_target_player_metadata(player_table, BOB_ADDRESS);
            let alice_meta = get_target_player_metadata(player_table, ALICE_ADDRESS);
            assert!(amount_won(bob_meta) == 3, 0);
            assert!(tickets_claimed(bob_meta) == 3, 0);
            assert!(amount_won(alice_meta) == 1, 0);
            assert!(tickets_claimed(alice_meta) == 1, 0);
            ts::return_shared(store);
        };
        test_scenario::end(test);
    }


    fun test_leaderboard_max_size_(test: Scenario) {
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            let number_of_prizes = vector<u64>[1, 1, 1];
            let value_of_prizes = vector<u64>[1, 1, 1];
            setup_test_sui_store(
                &mut test,
                OWNER_ADDRESS,
                number_of_prizes,
                value_of_prizes,
                3,
                MAX_LEADERBOARD_SIZE
            )
        };
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            // Send 1 ticket to alice and 1 ticket to bob
            let store: ConvenienceStore = ts::take_shared(&test);
            let store_cap: StoreCap = ts::take_from_sender(&test);
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            game::send_ticket(&store_cap, BOB_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            game::send_ticket(&store_cap, BOB_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            ts::return_to_sender(&test, store_cap);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, ALICE_ADDRESS);
        {
            let ticket: Ticket = ts::take_from_sender(&test);
            let store: ConvenienceStore = ts::take_shared(&test);
            let id = game::evaluate_ticket(ticket, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing(id, b"test", &mut store, ts::ctx(&mut test));
            let leaderboard = leaderboard(&store);
            let players_list = leaderboard_players(leaderboard);
            assert!(vector::length(&players_list) == 1, 0);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, BOB_ADDRESS);
        {
            let ticket: Ticket = ts::take_from_sender(&test);
            let store: ConvenienceStore = ts::take_shared(&test);
            let id = game::evaluate_ticket(ticket, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing(id, b"test", &mut store, ts::ctx(&mut test));
            let leaderboard = leaderboard(&store);
            let players_list = leaderboard_players(leaderboard);
            assert!(vector::length(&players_list) == 1, 0);
            // This is because owner should not change as bob would be the second person to climb to the leaderboard
            assert!(*vector::borrow(&players_list, 0) == ALICE_ADDRESS, 0);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, BOB_ADDRESS);
        {
            let ticket: Ticket = ts::take_from_sender(&test);
            let store: ConvenienceStore = ts::take_shared(&test);
            let id = game::evaluate_ticket(ticket, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing(id, b"test", &mut store, ts::ctx(&mut test));
            let leaderboard = leaderboard(&store);
            let players_list = leaderboard_players(leaderboard);
            assert!(vector::length(&players_list) == 1, 0);
            // This is because owner should not change
            assert!(*vector::borrow(&players_list, 0) == BOB_ADDRESS, 0);
            ts::return_shared(store);
        };
        test_scenario::end(test);
    }

    fun test_no_more_tickets_(test: Scenario) {
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            let number_of_prizes = vector<u64>[1, 1, 1];
            let value_of_prizes = vector<u64>[1, 2, 5];
            setup_test_sui_store(
                &mut test,
                OWNER_ADDRESS,
                number_of_prizes,
                value_of_prizes,
                8,
                MAX_LEADERBOARD_SIZE
            )
        };
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            // Send 3 tickets to alice
            let store: ConvenienceStore = ts::take_shared(&test);
            let store_cap: StoreCap = ts::take_from_sender(&test);
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            ts::return_to_sender(&test, store_cap);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            // Try to send one more ticket and fail
            let store: ConvenienceStore = ts::take_shared(&test);
            let store_cap: StoreCap = ts::take_from_sender(&test);
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            ts::return_to_sender(&test, store_cap);
            ts::return_shared(store);
        };
        test_scenario::end(test);
    }

    fun test_play_and_evaluate_(test: Scenario) {
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            let number_of_prizes = vector<u64>[1, 1, 1];
            let value_of_prizes = vector<u64>[1, 1, 1];
            setup_test_sui_store(
                &mut test,
                OWNER_ADDRESS,
                number_of_prizes,
                value_of_prizes,
                3,
                MAX_LEADERBOARD_SIZE
            )
        };
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            // Send 1 tickets to alice
            let store: ConvenienceStore = ts::take_shared(&test);
            let store_cap: StoreCap = ts::take_from_sender(&test);
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            ts::return_to_sender(&test, store_cap);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, ALICE_ADDRESS);
        {
            let store: ConvenienceStore = ts::take_shared(&test);
            assert!(prize_pool_balance(&store) == 3, 0);
            let ticket: Ticket = ts::take_from_sender(&test);
            let id = game::evaluate_ticket(ticket, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing(id, b"test", &mut store, ts::ctx(&mut test));
            assert!(tickets_left(&store) == 2, 0);
            assert!(prize_pool_balance(&store) == 2, 0);
            ts::return_shared(store);
        };
        test_scenario::end(test);
    }

    // This case touches the last element in an array of at least size 2 
    fun test_play_and_evaluate_multiple_prizes_(test: Scenario) {
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            let number_of_prizes = vector<u64>[1000, 500, 250, 10];
            let value_of_prizes = vector<u64>[1, 2, 10, 100];
            setup_test_sui_store(
                &mut test,
                OWNER_ADDRESS,
                number_of_prizes,
                value_of_prizes,
                1000 + 1000 + 2500 + 1000,
                MAX_LEADERBOARD_SIZE
            )
        };
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            // Send 1 tickets to alice
            let store: ConvenienceStore = ts::take_shared(&test);
            let store_cap: StoreCap = ts::take_from_sender(&test);
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, 1, ts::ctx(&mut test));
            ts::return_to_sender(&test, store_cap);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, ALICE_ADDRESS);
        {
            let ticket: Ticket = ts::take_from_sender(&test);
            let store: ConvenienceStore = ts::take_shared(&test);
            assert!(prize_pool_balance(&store) == 5500, 0);
            let id = game::evaluate_ticket(ticket, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing(id, b"test", &mut store, ts::ctx(&mut test));
            assert!(tickets_left(&store) == 1759, 0);

            assert!(prize_pool_balance(&store) == 5499, 0);
            ts::return_shared(store);
        };
        test_scenario::end(test);
    }

}