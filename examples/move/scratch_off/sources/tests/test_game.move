#[test_only]
module scratch_off::test_game {
    use scratch_off::game::{Self, ConvenienceStore, ENoTicketsLeft, StoreCap, Ticket,
     winning_tickets_left, leaderboard, prize_pool_balance, leaderboard_players};

    #[test_only] use sui::test_scenario::{Self, Scenario};
    #[test_only] use sui::coin::{mint_for_testing, burn_for_testing};
    use sui::test_scenario as ts;
    use sui::sui::SUI;
    use std::vector;

    const ALICE_ADDRESS: address = @0xACE;
    const BOB_ADDRESS: address = @0xACEB;
    const OWNER_ADDRESS: address = @123;
    const MAX_LEADERBOARD_SIZE: u64 = 1;

    #[test_only]
    public fun setup_test_sui_store(
        scenario: &mut Scenario,
        creator: address,
        number_of_prizes: vector<u64>,
        value_of_prizes: vector<u64>,
        max_tickets_issued: u64,
        max_leaderboard_size: u64,
    ) {
        ts::next_tx(scenario, creator);
        {
            let coin = mint_for_testing<SUI>(1000 * 100000000, ts::ctx(scenario));
            let leftover_coin = game::open_store<SUI>(
                coin,
                number_of_prizes,
                value_of_prizes,
                max_tickets_issued,
                b"key", // Fake public key
                max_leaderboard_size,
                ts::ctx(scenario)
            );
            burn_for_testing<SUI>(leftover_coin);
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
    fun test_leaderboard_max_size() {
        test_leaderboard_max_size_(scenario());
    }

    fun test_leaderboard_max_size_(test: Scenario) {
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            let number_of_prizes = vector<u64>[1, 1, 1];
            let value_of_prizes = vector<u64>[1,2,5];
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
            let store: ConvenienceStore<SUI> = ts::take_shared(&test);
            let store_cap: StoreCap = ts::take_from_sender(&test);
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, ts::ctx(&mut test));
            game::send_ticket(&store_cap, BOB_ADDRESS, &mut store, ts::ctx(&mut test));
            ts::return_to_sender(&test, store_cap);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, ALICE_ADDRESS);
        {
            let ticket: Ticket = ts::take_from_sender(&test);
            let store: ConvenienceStore<SUI> = ts::take_shared(&test);
            let id = game::evaluate_ticket<SUI>(ticket, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing<SUI>(id, b"test", &mut store, ts::ctx(&mut test));
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, BOB_ADDRESS);
        {
            let ticket: Ticket = ts::take_from_sender(&test);
            let store: ConvenienceStore<SUI> = ts::take_shared(&test);
            let id = game::evaluate_ticket<SUI>(ticket, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing<SUI>(id, b"test", &mut store, ts::ctx(&mut test));
            let leaderboard = leaderboard<SUI>(&store);
            let players_list = leaderboard_players(leaderboard);
            assert!(vector::length(&players_list) == 1, 0);
            
            ts::return_shared(store);

        };
        test_scenario::end(test);

    }

    fun test_no_more_tickets_(test: Scenario) {
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            let number_of_prizes = vector<u64>[1, 1, 1];
            let value_of_prizes = vector<u64>[1,2,5];
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
            // Send 3 tickets to alice
            let store: ConvenienceStore<SUI> = ts::take_shared(&test);
            let store_cap: StoreCap = ts::take_from_sender(&test);
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, ts::ctx(&mut test));
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, ts::ctx(&mut test));
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, ts::ctx(&mut test));
            ts::return_to_sender(&test, store_cap);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, OWNER_ADDRESS);
        {
            // Try to send one more ticket and fail
            let store: ConvenienceStore<SUI> = ts::take_shared(&test);
            let store_cap: StoreCap = ts::take_from_sender(&test);
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, ts::ctx(&mut test));
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
            let store: ConvenienceStore<SUI> = ts::take_shared(&test);
            let store_cap: StoreCap = ts::take_from_sender(&test);
            game::send_ticket(&store_cap, ALICE_ADDRESS, &mut store, ts::ctx(&mut test));
            ts::return_to_sender(&test, store_cap);
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, ALICE_ADDRESS);
        {
            let ticket: Ticket = ts::take_from_sender(&test);
            let store: ConvenienceStore<SUI> = ts::take_shared(&test);

            assert!(prize_pool_balance(&store) == 3, 0);

            let id = game::evaluate_ticket<SUI>(ticket, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing<SUI>(id, b"test", &mut store, ts::ctx(&mut test));
            assert!(winning_tickets_left(&store) == 2, 0);
            assert!(prize_pool_balance(&store) == 2, 0);
            ts::return_shared(store);
        };
        test_scenario::end(test);
    }

}