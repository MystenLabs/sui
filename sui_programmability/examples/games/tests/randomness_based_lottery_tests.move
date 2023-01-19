// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games::randomness_based_lottery_tests {
    use sui::test_scenario::{Self, Scenario};
    use games::randomness_based_lottery::{Self, Lottery, Ticket};
    use sui::sui::SUI;
    use sui::coin;
    use sui::transfer;
    use sui::coin::Coin;
    use sui::randomness::Randomness;
    use std::option;
    use sui::randomness;

    fun mint(addr: address, amount: u64, scenario: &mut Scenario) {
        transfer::transfer(coin::mint_for_testing<SUI>(amount, test_scenario::ctx(scenario)), addr);
        test_scenario::next_tx(scenario, addr);
    }

    #[test]
    fun test_play_randomness_lottery() {
        let user1 = @0x0;
        let user2 = @0x1;
        let user3 = @0x2;
        let user4 = @0x3;

        let scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        randomness_based_lottery::create(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let lottery = test_scenario::take_shared<Lottery>(scenario);

        // User1 buys a ticket.
        test_scenario::next_tx(scenario, user1);
        mint(user1, 1, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        test_scenario::next_tx(scenario, user1);
        randomness_based_lottery::buy_ticket(&mut lottery, coin, test_scenario::ctx(scenario));
        // User2 buys a ticket.
        test_scenario::next_tx(scenario, user2);
        mint(user2, 1, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        test_scenario::next_tx(scenario, user2);
        randomness_based_lottery::buy_ticket(&mut lottery, coin, test_scenario::ctx(scenario));
        // User3 buys a ticket
        test_scenario::next_tx(scenario, user3);
        mint(user3, 1, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        test_scenario::next_tx(scenario, user3);
        randomness_based_lottery::buy_ticket(&mut lottery, coin, test_scenario::ctx(scenario));
        // User4 buys a ticket
        test_scenario::next_tx(scenario, user4);
        mint(user4, 1, scenario);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        test_scenario::next_tx(scenario, user4);
        randomness_based_lottery::buy_ticket(&mut lottery, coin, test_scenario::ctx(scenario));

        // User 2 closes the game.
        test_scenario::next_tx(scenario, user2);
        randomness_based_lottery::close(&mut lottery, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user2);
        let r = test_scenario::take_shared<Randomness<randomness_based_lottery::RANDOMNESS_WITNESS>>(scenario);
        assert!(option::is_none(randomness::value(&r)), 0);
        let sig = randomness::sign(&r);
        randomness_based_lottery::determine_winner(&mut lottery, &mut r, sig);
        assert!(option::is_some(randomness::value(&r)), 0);

        // User 4 is the winner
        test_scenario::next_tx(scenario, user4);
        let ticket = test_scenario::take_from_sender<Ticket>(scenario);
        randomness_based_lottery::claim_prize(&mut lottery, ticket, test_scenario::ctx(scenario));

        test_scenario::return_shared(r);
        test_scenario::return_shared(lottery);
        test_scenario::end(scenario_val);
    }
}
