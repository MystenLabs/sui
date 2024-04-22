// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games::raffles_tests {
    use sui::clock;
    use sui::coin::{Self, Coin};
    use sui::random::{Self, update_randomness_state_for_testing, Random};
    use sui::sui::SUI;
    use sui::test_scenario::{Self, Scenario};
    use games::small_raffle;

    use games::raffle_with_tickets;

    fun mint(addr: address, amount: u64, scenario: &mut Scenario) {
        transfer::public_transfer(coin::mint_for_testing<SUI>(amount, test_scenario::ctx(scenario)), addr);
        test_scenario::next_tx(scenario, addr);
    }

    #[test]
    fun test_game_with_tickets() {
        let user1 = @0x0;
        let user2 = @0x1;
        let user3 = @0x2;
        let user4 = @0x3;

        let mut scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        // Setup randomness
        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let mut random_state = test_scenario::take_shared<Random>(scenario);
        update_randomness_state_for_testing(
            &mut random_state,
            0,
            x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
            test_scenario::ctx(scenario),
        );

        // Create the game and get back the output objects.
        mint(user1, 1000, scenario);
        let end_time = 100;
        raffle_with_tickets::create(end_time, 10, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let mut game = test_scenario::take_shared<raffle_with_tickets::Game>(scenario);
        assert!(raffle_with_tickets::get_cost_in_sui(&game) == 10, 1);
        assert!(raffle_with_tickets::get_participants(&game) == 0, 1);
        assert!(raffle_with_tickets::get_end_time(&game) == end_time, 1);
        assert!(raffle_with_tickets::get_winner(&game) == option::none(), 1);
        assert!(raffle_with_tickets::get_balance(&game) == 0, 1);

        let mut clock = clock::create_for_testing(test_scenario::ctx(scenario));
        clock::set_for_testing(&mut clock, 10);

        // Play with 4 users (everything here is deterministic)
        test_scenario::next_tx(scenario, user1);
        mint(user1, 10, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        let t1 = raffle_with_tickets::buy_ticket(&mut game, coin, &clock, test_scenario::ctx(scenario));
        assert!(raffle_with_tickets::get_participants(&game) == 1, 1);
        raffle_with_tickets::destroy_ticket(t1); // loser

        test_scenario::next_tx(scenario, user2);
        mint(user2, 10, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        let t2 = raffle_with_tickets::buy_ticket(&mut game, coin, &clock, test_scenario::ctx(scenario));
        assert!(raffle_with_tickets::get_participants(&game) == 2, 1);
        raffle_with_tickets::destroy_ticket(t2); // loser

        test_scenario::next_tx(scenario, user3);
        mint(user3, 10, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        let t3 = raffle_with_tickets::buy_ticket(&mut game, coin, &clock, test_scenario::ctx(scenario));
        assert!(raffle_with_tickets::get_participants(&game) == 3, 1);
        raffle_with_tickets::destroy_ticket(t3); // loser

        test_scenario::next_tx(scenario, user4);
        mint(user4, 10, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        let t4 = raffle_with_tickets::buy_ticket(&mut game, coin, &clock, test_scenario::ctx(scenario));
        assert!(raffle_with_tickets::get_participants(&game) == 4, 1);
        // this is the winner

        // Determine the winner (-> user3)
        clock::set_for_testing(&mut clock, 101);
        raffle_with_tickets::determine_winner(&mut game, &random_state, &clock, test_scenario::ctx(scenario));
        assert!(raffle_with_tickets::get_winner(&game) == option::some(4), 1);
        assert!(raffle_with_tickets::get_balance(&game) == 40, 1);
        clock::destroy_for_testing(clock);

        // Take the reward
        let coin = raffle_with_tickets::redeem(t4, game, test_scenario::ctx(scenario));
        assert!(coin::value(&coin) == 40, 1);
        coin::burn_for_testing(coin);

        test_scenario::return_shared(random_state);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_small_raffle() {
        let user1 = @0x0;
        let user2 = @0x1;
        let user3 = @0x2;
        let user4 = @0x3;

        let mut scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        // Setup randomness
        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let mut random_state = test_scenario::take_shared<Random>(scenario);
        update_randomness_state_for_testing(
            &mut random_state,
            0,
            x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
            test_scenario::ctx(scenario),
        );

        // Create the game and get back the output objects.
        mint(user1, 1000, scenario);
        let end_time = 100;
        small_raffle::create(end_time, 10, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let mut game = test_scenario::take_shared<small_raffle::Game>(scenario);
        assert!(small_raffle::get_cost_in_sui(&game) == 10, 1);
        assert!(small_raffle::get_participants(&game) == 0, 1);
        assert!(small_raffle::get_end_time(&game) == end_time, 1);
        assert!(small_raffle::get_balance(&game) == 0, 1);

        let mut clock = clock::create_for_testing(test_scenario::ctx(scenario));
        clock::set_for_testing(&mut clock, 10);

        // Play with 4 users (everything here is deterministic)
        test_scenario::next_tx(scenario, user1);
        mint(user1, 10, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        small_raffle::play(&mut game, coin, &clock, test_scenario::ctx(scenario));
        assert!(small_raffle::get_participants(&game) == 1, 1);

        test_scenario::next_tx(scenario, user2);
        mint(user2, 10, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        small_raffle::play(&mut game, coin, &clock, test_scenario::ctx(scenario));
        assert!(small_raffle::get_participants(&game) == 2, 1);

        test_scenario::next_tx(scenario, user3);
        mint(user3, 10, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        small_raffle::play(&mut game, coin, &clock, test_scenario::ctx(scenario));
        assert!(small_raffle::get_participants(&game) == 3, 1);

        test_scenario::next_tx(scenario, user4);
        mint(user4, 10, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        small_raffle::play(&mut game, coin, &clock, test_scenario::ctx(scenario));
        assert!(small_raffle::get_participants(&game) == 4, 1);

        // Determine the winner (-> user4)
        clock::set_for_testing(&mut clock, 101);
        small_raffle::close(game, &random_state, &clock, test_scenario::ctx(scenario));
        clock::destroy_for_testing(clock);

        // Check that received the reward
        test_scenario::next_tx(scenario, user4);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        assert!(coin::value(&coin) == 40, 1);
        coin::burn_for_testing(coin);

        test_scenario::return_shared(random_state);
        test_scenario::end(scenario_val);
    }
}
