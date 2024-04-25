// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games::drand_based_scratch_card_tests {
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::test_scenario::{Self, Scenario};

    use games::drand_based_scratch_card;

    fun mint(addr: address, amount: u64, scenario: &mut Scenario) {
        transfer::public_transfer(coin::mint_for_testing<SUI>(amount, scenario.ctx()), addr);
        scenario.next_tx(addr);
    }

    #[test]
    fun test_play_drand_scratch_card_with_winner() {
        let user1 = @0x0;
        let user2 = @0x1;

        let mut scenario = test_scenario::begin(user1);

        // Create the game and get back the output objects.
        mint(user1, 10, &mut scenario);
        let coin1 = scenario.take_from_sender<Coin<SUI>>();
        drand_based_scratch_card::create(coin1, 10, 10, scenario.ctx());
        scenario.next_tx(user1);
        let game = scenario.take_immutable<drand_based_scratch_card::Game>();
        let mut reward_val = scenario.take_shared<drand_based_scratch_card::Reward>();
        let drand_final_round = drand_based_scratch_card::end_of_game_round(game.get_game_base_drand_round());
        assert!(drand_final_round == 58810, 1);

        // Since everything here is deterministic, we know that the 49th ticket will be a winner.
        let mut i = 0;
        loop {
            // User2 buys a ticket.
            scenario.next_tx(user2);
            mint(user2, 1, &mut scenario);
            let coin2 = scenario.take_from_sender<Coin<SUI>>();
            drand_based_scratch_card::buy_ticket(coin2, &game, scenario.ctx());
            scenario.next_tx(user1);
            let coin1 = scenario.take_from_sender<Coin<SUI>>();
            assert!(coin1.value() == 1, 1);
            scenario.return_to_sender(coin1);
            scenario.next_tx(user2);
            let ticket = scenario.take_from_sender<drand_based_scratch_card::Ticket>();
            // Generated using:
            // curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/58810
            ticket.evaluate(
                &game,
                x"876b8586ed9522abd0ca596d6e214e9a7e9bedc4a2e9698d27970e892287268062aba93fd1a7c24fcc188a4c7f0a0e98",
                scenario.ctx()
            );
            scenario.next_tx(user2);
            if (scenario.has_most_recent_for_sender<drand_based_scratch_card::Winner>()) {
                break
            };
            i = i + 1;
        };
        // This value may change if the object ID is changed.
        assert!(i == 3, 1);

        // Claim the reward.
        let winner = scenario.take_from_sender<drand_based_scratch_card::Winner>();
        scenario.next_tx(user2);
        let reward = &mut reward_val;
        winner.take_reward(reward, scenario.ctx());
        scenario.next_tx(user2);
        let coin2 = scenario.take_from_sender<Coin<SUI>>();
        assert!(coin2.value() == 10, 1);
        scenario.return_to_sender(coin2);

        test_scenario::return_shared(reward_val);
        test_scenario::return_immutable(game);
        scenario.end();
    }

    #[test]
    fun test_play_drand_scratch_card_without_winner() {
        let user1 = @0x0;

        let mut scenario = test_scenario::begin(user1);

        // Create the game and get back the output objects.
        mint(user1, 10, &mut scenario);
        let coin1 = scenario.take_from_sender<Coin<SUI>>();
        drand_based_scratch_card::create(coin1, 10, 10, scenario.ctx());
        scenario.next_tx(user1);
        let game = scenario.take_immutable<drand_based_scratch_card::Game>();
        let mut reward = scenario.take_shared<drand_based_scratch_card::Reward>();

        // More 4 epochs forward.
        scenario.next_epoch(user1);
        scenario.next_epoch(user1);
        scenario.next_epoch(user1);
        scenario.next_epoch(user1);

        // Take back the reward.
        reward.redeem(&game, scenario.ctx());
        scenario.next_tx(user1);
        let coin1 = scenario.take_from_sender<Coin<SUI>>();
        assert!(coin1.value() == 10, 1);
        scenario.return_to_sender(coin1);

        test_scenario::return_shared(reward);
        test_scenario::return_immutable(game);
        scenario.end();
    }
}
