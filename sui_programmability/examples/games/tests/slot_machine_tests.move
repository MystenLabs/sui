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
        transfer::public_transfer(coin::mint_for_testing<SUI>(amount, test_scenario::ctx(scenario)), addr);
        test_scenario::next_tx(scenario, addr);
    }

    #[test]
    fun test_game() {
        let user1 = @0x0;
        let user2 = @0x1;
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
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        slot_machine::create(coin, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let mut game = test_scenario::take_shared<slot_machine::Game>(scenario);
        assert!(slot_machine::get_balance(&game) == 1000, 1);
        assert!(slot_machine::get_epoch(&game) == 0, 1);

        // Play 4 turns (everything here is deterministic)
        test_scenario::next_tx(scenario, user2);
        mint(user2, 100, scenario);
        let mut coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        slot_machine::play(&mut game, &random_state, &mut coin, test_scenario::ctx(scenario));
        assert!(slot_machine::get_balance(&game) == 1100, 1); // lost 100
        assert!(coin::value(&coin) == 0, 1);
        test_scenario::return_to_sender(scenario, coin);

        test_scenario::next_tx(scenario, user2);
        mint(user2, 200, scenario);
        let mut coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        slot_machine::play(&mut game, &random_state, &mut coin, test_scenario::ctx(scenario));
        assert!(slot_machine::get_balance(&game) == 900, 1); // won 200
        // check that received the right amount
        assert!(coin::value(&coin) == 400, 1);
        test_scenario::return_to_sender(scenario, coin);

        test_scenario::next_tx(scenario, user2);
        mint(user2, 300, scenario);
        let mut coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        slot_machine::play(&mut game, &random_state, &mut coin, test_scenario::ctx(scenario));
        assert!(slot_machine::get_balance(&game) == 600, 1); // won 300
        // check that received the remaining amount
        assert!(coin::value(&coin) == 600, 1);
        test_scenario::return_to_sender(scenario, coin);

        test_scenario::next_tx(scenario, user2);
        mint(user2, 200, scenario);
        let mut coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        slot_machine::play(&mut game, &random_state, &mut coin, test_scenario::ctx(scenario));
        assert!(slot_machine::get_balance(&game) == 800, 1); // lost 200
        // check that received the right amount
        assert!(coin::value(&coin) == 0, 1);
        test_scenario::return_to_sender(scenario, coin);

        // TODO: test also that the last coin is taken

        // Take remaining balance
        test_scenario::next_epoch(scenario, user1);
        let coin = slot_machine::close(game, test_scenario::ctx(scenario));
        assert!(coin::value(&coin) == 800, 1);
        coin::burn_for_testing(coin);

        test_scenario::return_shared(random_state);
        test_scenario::end(scenario_val);
    }
}
