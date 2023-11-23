#[test_only]
module scratch_off::test_game {
    use scratch_off::game::{Self, ConvenienceStore, ENoTicketsLeft, Ticket,
     winning_tickets_left, losing_tickets_left, prize_pool_balance};

    #[test_only] use sui::test_scenario::{Self, Scenario};
    #[test_only] use sui::coin::{mint_for_testing, burn_for_testing};
    use sui::test_scenario as ts;
    use sui::sui::SUI;

    #[test_only]
    public fun setup_test_sui_store(
        scenario: &mut Scenario,
        creator: address,
        number_of_prizes: vector<u64>,
        value_of_prizes: vector<u64>,
        max_tickets_issued: u64
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
                ts::ctx(scenario),
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

    fun test_no_more_tickets_(test: Scenario) {
        let owner: address = @0xF;
        let alice: address = @0xAAAA;
        ts::next_tx(&mut test, owner);
        {
            let number_of_prizes = vector<u64>[1, 1, 1];
            let value_of_prizes = vector<u64>[1,2,5];
            setup_test_sui_store(
                &mut test,
                owner,
                number_of_prizes,
                value_of_prizes,
                3
            )
        };
        ts::next_tx(&mut test, owner);
        {
            // Send 3 tickets to alice
            let store: ConvenienceStore<SUI> = ts::take_shared(&test);
            game::send_ticket(alice, &mut store, ts::ctx(&mut test));
            game::send_ticket(alice, &mut store, ts::ctx(&mut test));
            game::send_ticket(alice, &mut store, ts::ctx(&mut test));
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, owner);
        {
            // Try to send one more ticket and fail
            let store: ConvenienceStore<SUI> = ts::take_shared(&test);
            game::send_ticket(alice, &mut store, ts::ctx(&mut test));
            ts::return_shared(store);
        };
        test_scenario::end(test);
    }

    #[test]
    fun test_play_and_evaluate() {
        test_play_and_evaluate_(scenario());
    }

    fun test_play_and_evaluate_(test: Scenario) {
        let owner: address = @0xF;
        let alice: address = @0xAAAA;
        ts::next_tx(&mut test, owner);
        {
            let number_of_prizes = vector<u64>[1, 1, 1];
            let value_of_prizes = vector<u64>[1, 1, 1];
            setup_test_sui_store(
                &mut test,
                owner,
                number_of_prizes,
                value_of_prizes,
                3
            )
        };
        ts::next_tx(&mut test, owner);
        {
            // Send 3 tickets to alice
            let store: ConvenienceStore<SUI> = ts::take_shared(&test);
            game::send_ticket(alice, &mut store, ts::ctx(&mut test));
            ts::return_shared(store);
        };
        ts::next_tx(&mut test, alice);
        {
            let ticket: Ticket = ts::take_from_sender(&test);
            let store: ConvenienceStore<SUI> = ts::take_shared(&test);

            assert!(prize_pool_balance(&store) == 3, 0);

            let id = game::evaluate_ticket<SUI>(ticket, &mut store, ts::ctx(&mut test));
            game::finish_evaluation_for_testing<SUI>(id, b"test", &mut store, ts::ctx(&mut test));
            assert!(winning_tickets_left(&store) == 2, 0);
            assert!(losing_tickets_left(&store) == 0, 0);
            assert!(prize_pool_balance(&store) == 2, 0);
            ts::return_shared(store);
        };
        test_scenario::end(test);
    }

}