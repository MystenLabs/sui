// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games::slot_machine_tests {
    use sui::coin::{Self, Coin};
    use sui::random::{Self, update_randomness_state_for_testing, Random};
    use sui::sui::SUI;
    use sui::test_scenario::{Self, Scenario};
    use games::slot_machine;

    fun mint(addr: address, amount: u64, scenario: &mut Scenario) {
        transfer::public_transfer(coin::mint_for_testing<SUI>(amount, scenario.ctx()), addr);
        scenario.next_tx(addr);
    }

    #[test]
    fun test_game() {
        let user1 = @0x0;
        let user2 = @0x1;
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
        let coin = scenario.take_from_sender<Coin<SUI>>();
        slot_machine::create(coin, scenario.ctx());
        scenario.next_tx(user1);
        let mut game = scenario.take_shared<slot_machine::Game>();
        assert!(game.get_balance() == 1000, 1);
        assert!(game.get_epoch() == 0, 1);

        // Play 4 turns (everything here is deterministic)
        scenario.next_tx(user2);
        mint(user2, 100, scenario);
        let mut coin = scenario.take_from_sender<Coin<SUI>>();
        game.play(&random_state, &mut coin, scenario.ctx());
        assert!(game.get_balance() == 1100, 1); // lost 100
        assert!(coin.value() == 0, 1);
        scenario.return_to_sender(coin);

        scenario.next_tx(user2);
        mint(user2, 200, scenario);
        let mut coin = scenario.take_from_sender<Coin<SUI>>();
        game.play(&random_state, &mut coin, scenario.ctx());
        assert!(game.get_balance() == 900, 1); // won 200
        // check that received the right amount
        assert!(coin.value() == 400, 1);
        scenario.return_to_sender(coin);

        scenario.next_tx(user2);
        mint(user2, 300, scenario);
        let mut coin = scenario.take_from_sender<Coin<SUI>>();
        game.play(&random_state, &mut coin, scenario.ctx());
        assert!(game.get_balance() == 600, 1); // won 300
        // check that received the remaining amount
        assert!(coin.value() == 600, 1);
        scenario.return_to_sender(coin);

        scenario.next_tx(user2);
        mint(user2, 200, scenario);
        let mut coin = scenario.take_from_sender<Coin<SUI>>();
        game.play(&random_state, &mut coin, scenario.ctx());
        assert!(game.get_balance() == 800, 1); // lost 200
        // check that received the right amount
        assert!(coin.value() == 0, 1);
        scenario.return_to_sender(coin);

        // TODO: test also that the last coin is taken

        // Take remaining balance
        scenario.next_epoch(user1);
        let coin = game.close(scenario.ctx());
        assert!(coin.value() == 800, 1);
        coin.burn_for_testing();

        test_scenario::return_shared(random_state);
        scenario_val.end();
    }
}
