// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games::drand_based_scratch_card_tests {
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::test_scenario::{Self, Scenario};
    use sui::transfer;

    use games::drand_based_scratch_card;

    fun mint(addr: address, amount: u64, scenario: &mut Scenario) {
        transfer::transfer(coin::mint_for_testing<SUI>(amount, test_scenario::ctx(scenario)), addr);
        test_scenario::next_tx(scenario, addr);
    }

    #[test]
    fun test_play_drand_scratch_card_with_winner() {
        let user1 = @0x0;
        let user2 = @0x1;

        let scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        // Create the game and get back the output objects.
        mint(user1, 10, scenario);
        let coin1 = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        drand_based_scratch_card::create(coin1, 10, 10, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let game = test_scenario::take_immutable<drand_based_scratch_card::Game>(scenario);
        let reward_val = test_scenario::take_shared<drand_based_scratch_card::Reward>(scenario);
        let drand_final_round = drand_based_scratch_card::end_of_game_round(drand_based_scratch_card::get_game_base_drand_round(&game));
        assert!(drand_final_round == 5890, 1);

        // Since everything here is deterministic, we know that the 9th ticket will be a winner.
        let i = 0;
        while (i < 10) {
            // User2 buys a ticket.
            test_scenario::next_tx(scenario, user2);
            mint(user2, 1, scenario);
            let coin2 = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            drand_based_scratch_card::buy_ticket(coin2, &game, test_scenario::ctx(scenario));
            test_scenario::next_tx(scenario, user1);
            let coin1 = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            assert!(coin::value(&coin1) == 1, 1);
            test_scenario::return_to_sender(scenario, coin1);
            test_scenario::next_tx(scenario, user2);
            let ticket = test_scenario::take_from_sender<drand_based_scratch_card::Ticket>(scenario);
            // Generated using:
            // curl https://drand.cloudflare.com/8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce/public/5890
            drand_based_scratch_card::evaluate(
                ticket,
                &game,
                x"98a99ec46b8bda71fbb0a90a0c6a02e0be92803d756ff66b386fd7647a6071fc33116cd94b0a0f09b48d78e399b6ca590868213a2bb85be829841fdac9487f89b5ce02c2d13d38240d40ce9868bd17f903cf5fba5b825769cdbf38c22cebc6a5",
                x"b0ee0f3d50e7ed6d5860ce9addbada59fbde444745fedd46f8a15dd48f5c3524adeed06d54775b24bf652aadd087cf790a2f7c55bd81ea29f594d3c68a93bb5c3a595af0f1a368c762c07113f683abad50d7fceb1048e7376deb6febcb4683c6",
                test_scenario::ctx(scenario)
            );
            test_scenario::next_tx(scenario, user2);
            if (test_scenario::has_most_recent_for_sender<drand_based_scratch_card::Winner>(scenario)) {
                break
            };
            i = i + 1;
        };
        assert!(i == 8, 1);

        // Claim the reward.
        let winner = test_scenario::take_from_sender<drand_based_scratch_card::Winner>(scenario);
        test_scenario::next_tx(scenario, user2);
        let reward = &mut reward_val;
        drand_based_scratch_card::take_reward(winner, reward, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user2);
        let coin2 = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        assert!(coin::value(&coin2) == 10, 1);
        test_scenario::return_to_sender(scenario, coin2);

        test_scenario::return_shared(reward_val);
        test_scenario::return_immutable(game);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_play_drand_scratch_card_without_winner() {
        let user1 = @0x0;

        let scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        // Create the game and get back the output objects.
        mint(user1, 10, scenario);
        let coin1 = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        drand_based_scratch_card::create(coin1, 10, 10, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let game = test_scenario::take_immutable<drand_based_scratch_card::Game>(scenario);
        let reward_val = test_scenario::take_shared<drand_based_scratch_card::Reward>(scenario);

        // More 4 epochs forward.
        test_scenario::next_epoch(scenario, user1);
        test_scenario::next_epoch(scenario, user1);
        test_scenario::next_epoch(scenario, user1);
        test_scenario::next_epoch(scenario, user1);

        // Take back the reward.
        let reward = &mut reward_val;
        drand_based_scratch_card::redeem(reward, &game, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let coin1 = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        assert!(coin::value(&coin1) == 10, 1);
        test_scenario::return_to_sender(scenario, coin1);

        test_scenario::return_shared(reward_val);
        test_scenario::return_immutable(game);
        test_scenario::end(scenario_val);
    }
}
