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
        transfer::public_transfer(coin::mint_for_testing<SUI>(amount, scenario.ctx()), addr);
        scenario.next_tx(addr);
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
        random::create_for_testing(scenario.ctx());
        scenario.next_tx(user1);
        let mut random_state = scenario.take_shared<Random>();
        update_randomness_state_for_testing(
            &mut random_state,
            0,
            x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
            scenario.ctx(),
        );

        // Create the game and get back the output objects.
        mint(user1, 1000, scenario);
        let end_time = 100;
        raffle_with_tickets::create(end_time, 10, scenario.ctx());
        scenario.next_tx(user1);
        let mut game = scenario.take_shared<raffle_with_tickets::Game>();
        assert!(game.get_cost_in_sui() == 10, 1);
        assert!(game.get_participants() == 0, 1);
        assert!(game.get_end_time() == end_time, 1);
        assert!(game.get_winner() == option::none(), 1);
        assert!(game.get_balance() == 0, 1);

        let mut clock = clock::create_for_testing(scenario.ctx());
        clock.set_for_testing(10);

        // Play with 4 users (everything here is deterministic)
        scenario.next_tx(user1);
        mint(user1, 10, scenario);
        let coin = scenario.take_from_sender<Coin<SUI>>();
        let t1 = game.buy_ticket(coin, &clock, scenario.ctx());
        assert!(game.get_participants() == 1, 1);
        t1.destroy_ticket(); // loser

        scenario.next_tx(user2);
        mint(user2, 10, scenario);
        let coin = scenario.take_from_sender<Coin<SUI>>();
        let t2 = game.buy_ticket(coin, &clock, scenario.ctx());
        assert!(game.get_participants() == 2, 1);
        t2.destroy_ticket(); // loser

        scenario.next_tx(user3);
        mint(user3, 10, scenario);
        let coin = scenario.take_from_sender<Coin<SUI>>();
        let t3 = game.buy_ticket(coin, &clock, scenario.ctx());
        assert!(game.get_participants() == 3, 1);
        t3.destroy_ticket(); // loser

        scenario.next_tx(user4);
        mint(user4, 10, scenario);
        let coin = scenario.take_from_sender<Coin<SUI>>();
        let t4 = game.buy_ticket(coin, &clock, scenario.ctx());
        assert!(game.get_participants() == 4, 1);
        // this is the winner

        // Determine the winner (-> user3)
        clock.set_for_testing(101);
        game.determine_winner(&random_state, &clock, scenario.ctx());
        assert!(game.get_winner() == option::some(4), 1);
        assert!(game.get_balance() == 40, 1);
        clock.destroy_for_testing();

        // Take the reward
        let coin = t4.redeem(game, scenario.ctx());
        assert!(coin.value() == 40, 1);
        coin.burn_for_testing();

        test_scenario::return_shared(random_state);
        scenario_val.end();
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
        random::create_for_testing(scenario.ctx());
        scenario.next_tx(user1);
        let mut random_state = scenario.take_shared<Random>();
        update_randomness_state_for_testing(
            &mut random_state,
            0,
            x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
            scenario.ctx(),
        );

        // Create the game and get back the output objects.
        mint(user1, 1000, scenario);
        let end_time = 100;
        small_raffle::create(end_time, 10, scenario.ctx());
        scenario.next_tx(user1);
        let mut game = scenario.take_shared<small_raffle::Game>();
        assert!(game.get_cost_in_sui() == 10, 1);
        assert!(game.get_participants() == 0, 1);
        assert!(game.get_end_time() == end_time, 1);
        assert!(game.get_balance() == 0, 1);

        let mut clock = clock::create_for_testing(scenario.ctx());
        clock.set_for_testing(10);

        // Play with 4 users (everything here is deterministic)
        scenario.next_tx(user1);
        mint(user1, 10, scenario);
        let coin = scenario.take_from_sender<Coin<SUI>>();
        game.play(coin, &clock, scenario.ctx());
        assert!(game.get_participants() == 1, 1);

        scenario.next_tx(user2);
        mint(user2, 10, scenario);
        let coin = scenario.take_from_sender<Coin<SUI>>();
        game.play(coin, &clock, scenario.ctx());
        assert!(game.get_participants() == 2, 1);

        scenario.next_tx(user3);
        mint(user3, 10, scenario);
        let coin = scenario.take_from_sender<Coin<SUI>>();
        game.play(coin, &clock, scenario.ctx());
        assert!(game.get_participants() == 3, 1);

        scenario.next_tx(user4);
        mint(user4, 10, scenario);
        let coin = scenario.take_from_sender<Coin<SUI>>();
        game.play(coin, &clock, scenario.ctx());
        assert!(game.get_participants() == 4, 1);

        // Determine the winner (-> user4)
        clock.set_for_testing(101);
        game.close(&random_state, &clock, scenario.ctx());
        clock.destroy_for_testing();

        // Check that received the reward
        scenario.next_tx(user4);
        let coin = scenario.take_from_sender<Coin<SUI>>();
        assert!(coin.value() == 40, 1);
        coin.burn_for_testing();

        test_scenario::return_shared(random_state);
        scenario_val.end();
    }
}
