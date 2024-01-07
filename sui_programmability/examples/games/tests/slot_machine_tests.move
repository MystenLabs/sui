// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games::slot_machine_tests {
    use sui::coin::{Self, Coin};
    use sui::random::{Self, Random, advance_random};
    use sui::sui::SUI;
    use sui::test_scenario::{Self, Scenario};
    use sui::transfer;

    use games::slot_machine;

    fun mint(addr: address, amount: u64, scenario: &mut Scenario) {
        transfer::public_transfer(coin::mint_for_testing<SUI>(amount, test_scenario::ctx(scenario)), addr);
        test_scenario::next_tx(scenario, addr);
    }

    #[test]
    fun test_game() {
        let user0 = @0x0;
        let user1 = @0x1;
        let user2 = @0x2;
        let scenario_val = test_scenario::begin(user0);
        let scenario = &mut scenario_val;

        // Setup randomness
        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user0);
        let random = test_scenario::take_shared<Random>(scenario);
        advance_random(&mut random, test_scenario::ctx(scenario));

        // Create the game and get back the output objects.
        test_scenario::next_tx(scenario, user0);
        mint(user1, 1000, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        slot_machine::create(coin, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let game = test_scenario::take_shared<slot_machine::Game>(scenario);
        assert!(slot_machine::get_balance(&game) == 1000, 1);

        // Play 4 turns (everything here is deterministic)
        test_scenario::next_tx(scenario, user2);
        mint(user2, 100, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        let ticket = slot_machine::start_spin(&mut game, coin, &random, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user0);
        advance_random(&mut random, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user2);
        slot_machine::complete_spin(ticket, &mut game, &random, test_scenario::ctx(scenario));
        assert!(slot_machine::get_balance(&game) == 900, 1); // won 100
        // check that received the right amount
        test_scenario::next_tx(scenario, user2);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        assert!(coin::value(&coin) == 200, 1);
        test_scenario::return_to_sender(scenario, coin);

        test_scenario::next_tx(scenario, user2);
        mint(user2, 200, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        let ticket = slot_machine::start_spin(&mut game, coin, &random, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user0);
        advance_random(&mut random, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user2);
        slot_machine::complete_spin(ticket, &mut game, &random, test_scenario::ctx(scenario));
        assert!(slot_machine::get_balance(&game) == 1100, 1); // lost 200

        // now in parallel
        test_scenario::next_tx(scenario, user2);
        mint(user2, 100, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        let ticket1 = slot_machine::start_spin(&mut game, coin, &random, test_scenario::ctx(scenario));
        mint(user2, 200, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        let ticket2 = slot_machine::start_spin(&mut game, coin, &random, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user0);
        advance_random(&mut random, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user2);
        slot_machine::complete_spin(ticket1, &mut game, &random, test_scenario::ctx(scenario));
        assert!(slot_machine::get_balance(&game) == 1000, 1); // lost 100, but 200 are still locked
        slot_machine::complete_spin(ticket2, &mut game, &random, test_scenario::ctx(scenario));
        assert!(slot_machine::get_balance(&game) == 1000, 1); // won 200
        // check that received the right amount
        test_scenario::next_tx(scenario, user2);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        assert!(coin::value(&coin) == 400, 1);
        test_scenario::return_to_sender(scenario, coin);

        // Take remaining balance
        test_scenario::next_epoch(scenario, user1);
        let coin = slot_machine::withdraw(&mut game, test_scenario::ctx(scenario));
        assert!(coin::value(&coin) == 1000, 1);
        coin::burn_for_testing(coin);

        test_scenario::return_shared(game);
        test_scenario::return_shared(random);
        test_scenario::end(scenario_val);
    }
}
